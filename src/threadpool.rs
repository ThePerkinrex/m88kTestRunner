use std::{
    collections::VecDeque,
    sync::mpsc::{channel, Receiver, Sender, TryRecvError},
    thread::{self, JoinHandle, Thread},
};

use crate::tests::TestData;

pub struct ThreadPool<T, U> {
    // threads: Vec<ThreadData<T, U>>,
    job_distributing_thread: JoinHandle<Vec<U>>,
    // job: Box<dyn Job<T, U>>,
    enqueue_tx: Sender<Enqueue<T>>,
    update_rx: Receiver<Update<usize>>,
    status: Vec<(usize, String)>,
    n_finished: usize,
    n_sent: usize,
}

enum Enqueue<T> {
    Data(T),
    Finish,
}

impl<T, U> ThreadPool<T, U>
where
    T: Send + 'static + Name,
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
        let (enqueue_tx, enqueue_rx) = channel::<Enqueue<T>>();
        let (update_tx, update_rx) = channel();
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
                                update_tx.send(Update::Started(id, data.name())).unwrap();
                                t.send(data, id);
                                to_complete += 1;
                            } else {
                                waiting.push_back((data, id));
                            }
                            results.push(None);
                            // println!("{id} {to_complete} {}", waiting.len());
                            id += 1;
                            false
                        }
                        Ok(Enqueue::Finish) => {
                            // println!("finishing");
                            finishing = true;
                            false
                        }
                        Err(TryRecvError::Disconnected) => break,
                        Err(TryRecvError::Empty) => true,
                    }
                };

                // if finishing {
                // println!("FINISHING {} {}", to_complete, waiting.len())
                // }

                for t in &mut threads {
                    while let Some((data, id)) = t.try_recv() {
                        update_tx.send(Update::Finished(id)).unwrap();
                        // println!("Finished {id}");
                        results[id] = Some(data);
                        to_complete -= 1;
                        if !t.working {
                            if let Some((data, id)) = waiting.pop_front() {
                                update_tx.send(Update::Started(id, data.name())).unwrap();
                                t.send(data, id);
                                to_complete += 1;
                            }
                        }
                    }
                }

                if finishing && to_complete == 0 && waiting.is_empty() {
                    // println!("queue len: {}", waiting.len());
                    // println!("FINISHING");
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
            update_rx,
            status: Vec::with_capacity(thread_n),
            n_finished: 0,
            n_sent: 0,
        }
    }

    pub fn send_data(&mut self, data: T) {
        self.n_sent += 1;
        self.enqueue_tx.send(Enqueue::Data(data)).unwrap();
    }

    pub fn update_status(&mut self) -> UpdatedStatus {
        let mut changed = false;
        while let Ok(data) = self.update_rx.try_recv() {
            changed = true;
            match data {
                Update::Started(id, name) => self.status.push((id, name)),
                Update::Finished(id) => {
                    self.n_finished += 1;
                    self.status.retain(|(id2, _)| &id != id2)
                }
            }
        }

        if changed {
            UpdatedStatus::Changed(self.status.iter().map(|(_, n)| n.clone()).collect())
        } else {
            UpdatedStatus::Unchanged
        }
    }

    pub fn finish(&self) {
        self.enqueue_tx.send(Enqueue::Finish).unwrap();
    }

    pub const fn is_finished(&self) -> FinishStatus {
        // println!("{} {}", self.n_sent, self.n_finished);
        if self.n_sent == 0 {
            FinishStatus::NotStarted
        } else if self.n_finished == self.n_sent {
            FinishStatus::Finished
        } else {
            FinishStatus::Working
        }
    }

    pub fn results(self) -> Vec<U> {
        self.job_distributing_thread.thread().unpark();
        self.job_distributing_thread.join().unwrap()
    }
}

struct ThreadData<T, U, Id> {
    t: JoinHandle<()>,
    tx: Sender<(Thread, T, Id)>,
    rx: Receiver<(U, Id)>,
    working: bool,
}

impl<T, U, Id: std::fmt::Display> ThreadData<T, U, Id> {
    fn send(&mut self, t: T, id: Id) {
        // TODO if working fail
        // println!("Starting {id}");
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

pub trait Name {
    fn name(&self) -> String;
}

impl Name for (usize, String, String, TestData) {
    fn name(&self) -> String {
        format!("{}/{}", self.1, self.2)
    }
}

enum Update<Id> {
    Started(Id, String),
    Finished(Id),
}

pub enum UpdatedStatus {
    Changed(Vec<String>),
    Unchanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishStatus {
    NotStarted,
    Working,
    Finished,
}
