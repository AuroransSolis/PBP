use std::io::{prelude::*, stdout, stdin};
use std::net::{TcpListener, TcpStream};
use std::mem::transmute_copy;

const MAX_X: u64 = 1000;
const WAIT_TIME: u32 = 15;

fn main() {
    let listener = TcpListener::bind("127.0.0.1:1337").unwrap();
    let mut tcpstreams = Vec::new();
    let mut timer = std::time::Instant::now();
    let mut iterator = (2..MAX_X).map(|x| x * x).filter(|&x| x % 24 == 1);
    'main_loop: loop {
        if let Ok((tcpstream, _)) = listener.accept() {
            tcpstreams.push(tcpstream);
        }
        for (i, tcpstream) in tcpstreams.iter_mut().enumerate() {
            let mut recv_data_type: [u8; 1] = [0];
            if let Err(_) = tcpstream.read_exact(&mut recv_data_type) {
                continue;
            }
            let recv_data_type = unsafe { transmute_copy::<[u8; 1], u8>(&recv_data_type) };
            match recv_data_type {
                0 => { // Work request
                    let new_x
                },
                1 => { // At: (u64, u64, u64)

                },
                2 => { // Solution: (u64, u64, u64)

                },
                _ => {}
            }
        }
    }
}