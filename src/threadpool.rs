use std::{
    collections::VecDeque,
    sync::mpsc::{channel, Receiver, Sender, TryRecvError},
    thread::{self, JoinHandle, Thread},
};

pub struct ThreadPool<T, U> {
    // threads: Vec<ThreadData<T, U>>,
    job_distributing_thread: JoinHandle<Vec<U>>,
    // job: Box<dyn Job<T, U>>,
    enqueue_tx: Sender<Enqueue<T>>,
}

enum Enqueue<T> {
    Data(T),
    Finish,
}

impl<T, U> ThreadPool<T, U>
where
    T: Send + 'static,
    U: Send + 'static,
{
    pub fn new<J: Job<T, U> + 'static>(job: J, thread_n: usize) -> Self {
        Self::new_boxed(Box::new(job), thread_n)
    }

    pub fn new_boxed(job: Box<dyn Job<T, U>>, thread_n: usize) -> Self {
        let mut threads = Vec::<ThreadData<T, U, usize>>::with_capacity(thread_n);
        for _ in 0..thread_n {
            let (tx1, rx1) = channel::<(Thread, T, _)>();
            let (tx2, rx2) = channel();
            let job = job.clone();
            let thread = thread::spawn(move || loop {
                match rx1.try_recv() {
                    Ok((job_thread, data, id)) => {
                        tx2.send((job.call(data, id), id)).unwrap();
                        job_thread.unpark();
                    }
                    Err(TryRecvError::Disconnected) => break,
                    Err(TryRecvError::Empty) => thread::park(),
                };
            });
            threads.push(ThreadData {
                t: thread,
                tx: tx1,
                rx: rx2,
                working: false,
            })
        }
        let (enqueue_tx, enqueue_rx) = channel();
        let job_distributing_thread = thread::spawn(move || {
            let mut id = 0;
            let mut waiting = VecDeque::new();
            let mut results = vec![];
            let mut finishing = false;
            let mut to_complete = 0;
            loop {
                let will_park = if finishing {
                    true
                } else {
                    match enqueue_rx.try_recv() {
                        Ok(Enqueue::Data(data)) => {
                            if let Some(t) = threads.iter_mut().find(|t| !t.working) {
                                t.send(data, id);
                            } else {
                                waiting.push_back((data, id));
                            }
                            results.push(None);
                            id += 1;
                            to_complete += 1;
                            false
                        }
                        Ok(Enqueue::Finish) => {
                            finishing = true;
                            false
                        }
                        Err(TryRecvError::Disconnected) => break,
                        Err(TryRecvError::Empty) => true,
                    }
                };

                for t in &mut threads {
                    while let Some((data, id)) = t.try_recv() {
                        results[id] = Some(data);
                        to_complete -= 1;
                        if !t.working {
                            if let Some((data, id)) = waiting.pop_front() {
                                t.send(data, id);
                            }
                        }
                    }
                }

                if finishing && to_complete == 0 {
                    break;
                }

                if will_park {
                    thread::park()
                }
            }
            results.into_iter().map(Option::unwrap).collect()
        });
        Self {
            job_distributing_thread,
            enqueue_tx,
        }
    }

    pub fn send_data(&self, data: T) {
        self.enqueue_tx.send(Enqueue::Data(data)).unwrap();
    }

    pub fn results(self) -> Vec<U> {
        self.enqueue_tx.send(Enqueue::Finish).unwrap();
        self.job_distributing_thread.join().unwrap()
    }
}

struct ThreadData<T, U, Id> {
    t: JoinHandle<()>,
    tx: Sender<(Thread, T, Id)>,
    rx: Receiver<(U, Id)>,
    working: bool,
}

impl<T, U, Id> ThreadData<T, U, Id> {
    fn send(&mut self, t: T, id: Id) {
        // TODO if working fail
        self.tx.send((thread::current(), t, id)).unwrap();
        self.t.thread().unpark();
        self.working = true;
    }

    fn try_recv(&mut self) -> Option<(U, Id)> {
        if self.working {
            let res = self.rx.try_recv().ok();
            if res.is_some() {
                self.working = false;
            }
            res
        } else {
            None
        }
    }
}

pub trait Job<T, U, Id = usize>: Send {
    fn call(&self, t: T, id: Id) -> U;
    fn clone(&self) -> Box<dyn Job<T, U, Id> + Send>;
}

impl<T, U, Id, F> Job<T, U, Id> for F
where
    F: Fn(T, Id) -> U + Clone + Send + 'static,
{
    fn call(&self, t: T, id: Id) -> U {
        self(t, id)
    }

    fn clone(&self) -> Box<dyn Job<T, U, Id> + Send> {
        Box::new(self.clone())
    }
}
