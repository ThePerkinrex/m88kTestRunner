use std::{
    sync::mpsc::{channel, Receiver, Sender, TryRecvError},
    thread::{self, JoinHandle, Thread},
};

pub struct ThreadPool<T> {
    // threads: Vec<ThreadData<T, U>>,
    job_distributing_thread: JoinHandle<()>,
    // job: Box<dyn Job<T, U>>,
    enqueue_tx: Sender<T>,
}

impl<T> ThreadPool<T>
where
    T: Send + 'static,
{
    pub fn new<U>(job: Box<dyn Job<T, U>>, thread_n: usize) -> Self
    where
        U: Send + 'static,
    {
        let mut threads = Vec::<ThreadData<T, U>>::with_capacity(thread_n);
        for _ in 0..thread_n {
            let (tx1, rx1) = channel::<(Thread, T)>();
            let (tx2, rx2) = channel();
            let job = job.clone();
            let thread = thread::spawn(move || loop {
                match rx1.try_recv() {
                    Ok((job_thread, data)) => {
                        tx2.send(job.call(data)).unwrap();
                        job_thread.unpark();
                    }
                    Err(TryRecvError::Disconnected) => break,
                    Err(TryRecvError::Empty) => (),
                };
                thread::park();
            });
            threads.push(ThreadData {
                t: thread,
                tx: tx1,
                rx: rx2,
                working: false,
            })
        }
        let job_distributing_thread = thread::spawn(move || {});
        todo!()
    }
}

struct ThreadData<T, U> {
    t: JoinHandle<()>,
    tx: Sender<(Thread, T)>,
    rx: Receiver<U>,
    working: bool,
}

impl<T, U> ThreadData<T, U> {
    fn send(&mut self, t: T) {
        // TODO if working fail
        self.tx.send((thread::current(), t)).unwrap();
        self.t.thread().unpark();
        self.working = true;
    }

    fn try_recv(&mut self) -> Option<U> {
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

pub trait Job<T, U>: Send {
    fn call(&self, t: T) -> U;
    fn clone(&self) -> Box<dyn Job<T, U> + Send>;
}

impl<T, U, F> Job<T, U> for F
where
    F: Fn(T) -> U + Clone + Send + 'static,
{
    fn call(&self, t: T) -> U {
        self(t)
    }

    fn clone(&self) -> Box<dyn Job<T, U> + Send> {
        Box::new(self.clone())
    }
}
