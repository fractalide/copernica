extern crate lru_cache;

use lru_cache::LruCache;
use packets::{Interest, Data};
use faces::{Face, Mock};

#[derive(Clone)]
pub struct Router<'a> {
    faces: Vec<&'a dyn Face>,
    cs: LruCache<String, String>,
    pit: bool,
    fib: bool
}

impl<'a> Router<'a> {
    pub fn new() -> Self {
        Router {
            faces: Vec::new(),
            cs: LruCache::new(10),
            pit: false,
            fib: false,
        }
    }

    pub fn add_face(&mut self, face: &'a dyn Face) {
        self.faces.push(face);
    }

    pub fn run(self) {
        self.faces[1].interest_in(self.faces[0].interest_out());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use crossbeam_channel::{unbounded, Sender, Receiver};

    #[test]
    fn setup_router_and_ensure_faces_still_operate_does_not_pass_ownership_into_router() {
        let f1: Mock = Face::new();
        let mut router = Router::new();
        router.add_face(&f1);
        let interest = Interest::new("interest".to_string());
        f1.interest_in(interest.clone());
        assert_eq!(interest, f1.interest_out());
    }

    #[test]
    fn test_throughput() {
        let f1: Mock = Face::new();
        let f2: Mock = Face::new();
        let f3: Mock = Face::new();
        let f4: Mock = Face::new();
        let mut r1 = Router::new();
        let mut r2 = Router::new();
        r1.add_face(&f1);
        r1.add_face(&f2);
        r2.add_face(&f3);
        r2.add_face(&f4);
        let interest = Interest::new("interest".to_string());
        let (i_in, i_out) = unbounded();
        f1.interest_in(interest.clone());
        r1.run();
        i_in.send(f2.interest_out()).unwrap();
        f3.interest_in(i_out.recv().unwrap());
        r2.run();
        assert_eq!(interest, f4.interest_out());
        // i -> f1 r1 f2 -> f3 r2 f4

    }
}
