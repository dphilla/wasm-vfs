// my_cmp/mod.rs
#![allow(dead_code)]

pub fn min(a: usize, b: usize) -> usize {
    if a < b { a } else { b }
}

pub fn max(a: usize, b: usize) -> usize {
    if a > b { a } else { b }
}

