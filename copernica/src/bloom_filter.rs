use {
    rand::Rng,
    bitvec::prelude::*,
    crate::{
        copernica_constants,
        hbfi::{HBFI}
    },
};

#[derive(Debug, Clone)]
pub struct BloomFilter {
    sdr: BitVec,
}

impl BloomFilter {
    #[allow(dead_code)]
    pub fn new() -> Self {
        BloomFilter {
            sdr: bitvec![0; copernica_constants::BLOOM_FILTER_LENGTH as usize],
        }
    }

    pub fn insert(&mut self, packet: &HBFI) {
        for id in &packet.id[..] {
            self.sdr.set(*id as usize, true);
        }
        for h1 in &packet.h1[..]{
            self.sdr.set(*h1 as usize, true);
        }
    }

    #[allow(dead_code)]
    pub fn contains(&self, packet: &HBFI) -> u8 {
        let mut sdr_vals: Vec<u32> = Vec::new();
        for id in &packet.id[..] {
            sdr_vals.push(*self.sdr.get(*id as usize).unwrap() as u32);
        }
        for h1 in &packet.h1[..]{
            sdr_vals.push(*self.sdr.get(*h1 as usize).unwrap() as u32);
        }
        let hits = sdr_vals.iter().try_fold(0u32, |acc, &elem| acc.checked_add(elem));
        let percentage = (hits.unwrap() as f32 / sdr_vals.len() as f32) * 100f32;
        //println!("hits: {:?}, length: {:?}, percentage: {}", hits.unwrap(), vals.len(), percentage);
        percentage as u8
    }

    #[allow(dead_code)]
    pub fn delete(&mut self, packet: &HBFI) {
        for id in &packet.id[..] {
            self.sdr.set(*id as usize, false);
        }
        for h1 in &packet.h1[..]{
            self.sdr.set(*h1 as usize, false);
        }
    }

    #[allow(dead_code)]
    pub fn partially_forget(&mut self) {
        let mut rng = rand::thread_rng();
        for _ in 0 .. 2048 {
            self.sdr.set(rng.gen_range(0, 2048), false);
        }
    }

    #[allow(dead_code)]
    pub fn decoherence(&self) -> u8 {
        let hits = self.sdr.iter().try_fold(0u32, |acc, elem| acc.checked_add(*elem as u32));
        let percentage = (hits.unwrap() as f32 / self.sdr.len() as f32) * 100f32;
        //println!("hits: {:?}, length: {:?}, percentage: {}", hits.unwrap(), vals.len(), percentage);
        percentage as u8
    }

}

impl PartialEq for BloomFilter {
    fn eq(&self, other: &BloomFilter) -> bool {
        self.sdr == other.sdr
    }
}
