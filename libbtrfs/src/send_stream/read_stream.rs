use super::handler::{
    StreamHandler,
    command::{EndCmd, SendCmd},
};
use crate::util::IoResult;
use rtrb::{PopError, RingBuffer};
use std::{
    io::Read,
    mem,
    range::Range,
    sync::Arc,
    sync::atomic::{AtomicU64, Ordering},
    thread,
};

#[cfg(all(test, feature = "use-crc-fast"))]
#[test]
pub fn check_crc32_params()
{
    let parms = SendStream::crc32_parms();
    let checksum = crc_fast::checksum_with_params(parms, b"123456789");

    assert_eq!(checksum, parms.check);
}

#[repr(C, packed)]
struct StreamHeader
{
    magic: [u8; SendStream::MAGIC.len()],
    version: u32,
}

#[repr(C, packed)]
struct CmdHeader
{
    length: u32,
    command: u16,
    checksum: u32,
}

pub struct SendStream<R>
{
    version: u32,
    command: u16,
    reader: R,
    buf: Vec<u8>,
    data: Range<usize>,
    // end of last successful read, equivalent to start of current malformed part of block
    stream_pos: u64,
    // updated after successful reads on a 4M interval
    atomic_pos: Option<Arc<AtomicU64>>,

    #[cfg(feature = "use-crc-fast")]
    crc_params: crc_fast::CrcParams,
}

impl SendStream<()>
{
    const MAGIC: &[u8; 13] = b"btrfs-stream\0";
    const SUPPORTED_VERSION: u32 = 2;

    // In send stream v1, no command is larger than 64KiB. In send stream v2, no limit should be assumed.
    const BUF_SZ_V1: usize = 64 * 1024;

    #[cfg(feature = "use-crc-fast")]
    fn crc32_parms() -> crc_fast::CrcParams
    {
        crc_fast::CrcParams::new(
            "BTRFS-CRC",
            32,
            0x1EDC6F41,
            0x00000000,
            true,
            0x00000000,
            0x58E3FA20,
        )
    }
}

impl<R: Read> SendStream<R>
{
    pub fn new(reader: R, atomic_pos: Option<Arc<AtomicU64>>) -> Self
    {
        Self {
            reader,
            atomic_pos,
            buf: vec![0; SendStream::BUF_SZ_V1],
            data: Range { start: size_of::<CmdHeader>(), end: 0 },
            version: 0,
            stream_pos: 0,
            command: 0,
            #[cfg(feature = "use-crc-fast")]
            crc_params: SendStream::crc32_parms(),
        }
    }

    /// Read and handle all send commands with the provided `handler`
    pub fn read_and_handle<H: StreamHandler>(&mut self, mut handler: H) -> IoResult<()>
    {
        self.read_header()?;
        loop {
            self.read_cmd()?;

            let data = &self.buf[self.data];
            let command = self.command;
            let version = self.version;

            if handler.handle_cmd(command, data, version)?.is_none() {
                break;
            }
        }

        if let Some(ref atomic) = self.atomic_pos {
            atomic.store(self.stream_pos, Ordering::Relaxed);
        }

        Ok(())
    }

    /// Buffered read and handle using rtrb::RingBuffer channels
    pub fn read_and_handle_buffered<H: StreamHandler>(&mut self, mut handler: H) -> IoResult<()>
    {
        // NOTE: this function is mainly helpful with a very fast reader. For example reading from a
        // send-stream that has been saved on disk. To get the fastest reader, wrap a File that
        // points to a send stream in a BufReader with a very large capacity.
        //
        // let cap = 1024 * 1024 * 4;
        // let file = std::fs::File::open("./send-stream-on-disk").unwrap();
        // let buf = std::io::BufReader::with_capacity(cap, file);
        // libbtrfs::send_stream::receive_stream("/subvol/destination", buf, None, true).unwrap();
        //
        //

        // More than two buffer only helps when the reader is waiting more than the IO thread
        // and when disk IO is very incosistent, so that the reader thread does not need to wait to
        // read from the stream if any single IO operation is taking a very long time
        let buf_count_args = 2;

        // clamp number of buffer from 2 to 4.
        let nbuf = buf_count_args.clamp(2, 4);

        self.read_header()?;

        thread::scope(|scope| {
            const THREAD_MAX_SPINS: usize = 40_000;
            const MAIN_MAX_SPINS: usize = 40_000;

            let version = self.version;

            let (mut s_data, mut r_data) = RingBuffer::<(u16, Range<usize>, Vec<u8>)>::new(nbuf);
            let (mut s_free, mut r_free) = RingBuffer::<IoResult<Vec<u8>>>::new(nbuf);

            for _ in 0..(nbuf - 1) {
                // prime the channel so calls to r_free.pop() will not be empty.
                // need 1 less than `nbuf` additional buffers.
                s_free.push(Ok(self.buf.clone())).unwrap();
            }

            scope.spawn(move || {
                //let mut thread_wait_time = std::time::Duration::ZERO;
                //let mut start = std::time::Instant::now();

                let mut spins = 0;
                loop {
                    match r_data.pop() {
                        Ok((cmd, payload, buf)) => {
                            //thread_wait_time += start.elapsed();

                            if let Err(e) = handler.handle_cmd(cmd, &buf[payload], version) {
                                let _ = s_free.push(Err(e));
                                break;
                            }

                            if s_free.push(Ok(buf)).is_err() {
                                break;
                            }

                            //start = std::time::Instant::now();

                            spins = 0;
                        }
                        Err(PopError::Empty) => {
                            if spins >= THREAD_MAX_SPINS {
                                if r_data.is_abandoned() {
                                    // main thread got the END cmd
                                    break;
                                }
                                thread::yield_now();
                            } else {
                                std::hint::spin_loop();
                                spins += 1;
                            }
                        }
                    }
                }

                //eprintln!("IO Thread wait time: {thread_wait_time:.2?}");
            });

            //let mut main_wait_time = std::time::Duration::ZERO;

            while self.command != EndCmd::KEY {
                self.read_cmd()?;

                s_data
                    .push((self.command, self.data, mem::take(&mut self.buf)))
                    .expect("Handler panicked");

                //let start = std::time::Instant::now();

                let mut spins = 0;
                'spin: loop {
                    match r_free.pop() {
                        Ok(result) => {
                            self.buf = result?;
                            break 'spin;
                        }
                        Err(PopError::Empty) => {
                            if spins >= MAIN_MAX_SPINS {
                                if r_free.is_abandoned() {
                                    // returning Ok(()) would just panic anyway when we join
                                    panic!("Handler thread panicked");
                                }
                                thread::yield_now();
                            } else {
                                std::hint::spin_loop();
                                spins += 1;
                            }
                        }
                    }
                }

                //main_wait_time += start.elapsed();
            }

            //eprintln!("Reader Thread wait time: {main_wait_time:.2?}");

            if let Some(ref atomic) = self.atomic_pos {
                atomic.store(self.stream_pos, Ordering::Relaxed);
            }

            Ok(())
        })
    }

    /*
     * Another implementation of using another Vec buffer to receive a btrfs send-stream.
     * Uses a Mutex and an AtomicBool instead of a channel. Probably not as good as using a rtrb
     * channel but better than using a `mpsc::channel` since that will immediately park the thread
     * while it is waiting.
     *
     * Keeping it around for now becuase there is still alot of work and tuning that need to be
     * done to get the best performance for receiving a btrfs send stream. Currently there is not
     * much of a performance increaese over a non buffered receive.
     *
    pub fn read_and_handle_buffered<H: StreamHandler>(&mut self, mut handler: H)
    -> IoResult<()>
    {
        self.read_header()?;

        struct Shared
        {
            buf: Vec<u8>,
            payload: Range<usize>,
            cmd: u16,
        }

        let version = self.version;
        let cmd_is_stale = std::sync::atomic::AtomicBool::new(true);
        let cmd_data = std::sync::Mutex::new(Shared {
            buf: vec![0; SendStream::<R>::BUF_SZ_V1],
            payload: self.payload,
            cmd: self.cmd,
        });

        thread::scope(|env| {
            env.spawn(|| -> IoResult<()> {
                //let mut io_wait_time = std::time::Duration::ZERO;

                loop {
                    //let start = std::time::Instant::now();

                    while cmd_is_stale.load(Ordering::Acquire) {
                        std::thread::yield_now();
                        std::hint::spin_loop();
                    }
                    {
                        //io_wait_time += start.elapsed();

                        let guard = cmd_data.lock().unwrap();

                        let data = &guard.buf[guard.payload];
                        let cmd = guard.cmd;

                        if handler.handle_cmd(cmd, data, version)?.is_none() {
                            break;
                        }
                    }

                    cmd_is_stale.store(true, Ordering::Release)
                }

                //eprintln!("IO Thread wait time: {io_wait_time:.2?}");

                Ok(())
            });

            //let mut reader_wait_time = std::time::Duration::ZERO;

            while self.cmd != EndCmd::KEY {
                self.read_cmd()?;

                //let start = std::time::Instant::now();

                while !cmd_is_stale.load(Ordering::Acquire) {
                    std::thread::yield_now();
                    std::hint::spin_loop();
                }
                {
                    //reader_wait_time += start.elapsed();

                    let mut guard = cmd_data.lock().unwrap();

                    guard.cmd = self.cmd;
                    guard.payload = self.payload;
                    mem::swap(&mut guard.buf, &mut self.buf);

                    cmd_is_stale.store(false, Ordering::Release);
                }
            }

            //eprintln!("Reader Thread wait time: {reader_wait_time:.2?}");

            Ok(())
        })
    }
    */

    fn read_header(&mut self) -> IoResult<()>
    {
        self.reader
            .read_exact(&mut self.buf[..size_of::<StreamHeader>()])?;

        let (magic, version) =
            self.buf[..size_of::<StreamHeader>()].split_at(SendStream::MAGIC.len());

        if magic != SendStream::MAGIC {
            return receive_error!("Unexpected header");
        }
        let version = u32::from_le_bytes(version.try_into().unwrap());

        if version > SendStream::SUPPORTED_VERSION {
            return receive_error!("Unsuppored Version");
        }
        self.version = version;

        Ok(())
    }

    fn read_cmd(&mut self) -> IoResult<()>
    {
        // update the atomic position on a 4M interval
        const BLOCK_MASK: u64 = !0x3F_FFFF;

        self.data.end = 0;
        self.reader
            .read_exact(&mut self.buf[..size_of::<CmdHeader>()])?;

        let hdr = unsafe {
            let p = self.buf.as_mut_ptr().cast::<CmdHeader>();
            let hdr = CmdHeader {
                length: u32::from_le((&raw const (*p).length).read_unaligned()),
                command: u16::from_le((&raw const (*p).command).read_unaligned()),
                checksum: u32::from_le((&raw const (*p).checksum).read_unaligned()),
            };
            (*p).checksum = 0;

            hdr
        };
        self.data.end = self.data.start + hdr.length as usize;

        if self.data.end > self.buf.len() {
            self.buf.resize(self.data.end, 0);
        }
        self.reader.read_exact(&mut self.buf[self.data])?;

        let checksum = {
            #[cfg(feature = "use-crc-fast")]
            {
                self.crc_params.init = 0;

                crc_fast::checksum_with_params(self.crc_params, &self.buf[..self.data.end]) as u32
            }
            #[cfg(not(feature = "use-crc-fast"))]
            !crc32c::crc32c_append(u32::MAX, &self.buf[..self.data.end])
        };

        if hdr.checksum != checksum {
            return receive_error!("crc mismatch in command");
        }

        let old_pos = self.stream_pos;
        self.stream_pos += self.data.end as u64;

        if let Some(ref atomic) = self.atomic_pos {
            if (old_pos & BLOCK_MASK) != (self.stream_pos & BLOCK_MASK) {
                // update the atomic progress every 4M
                atomic.store(self.stream_pos, Ordering::Relaxed)
            }
        }
        self.command = hdr.command;

        Ok(())
    }
}
