/// Raw Bluetooth RFCOMM socket connection using libc.
///
/// NixOS Python doesn't export AF_BLUETOOTH, so we use raw syscalls
/// via libc — same approach works in Rust natively.
use std::io::{self, Read, Write};
use std::os::unix::io::{AsRawFd, RawFd};
use std::time::Duration;

const AF_BLUETOOTH: i32 = 31;
const BTPROTO_RFCOMM: i32 = 3;
const SOL_RFCOMM: i32 = 18;
const RFCOMM_LM: i32 = 3;
const RFCOMM_LM_AUTH: u32 = 0x0002;
const RFCOMM_LM_ENCRYPT: u32 = 0x0004;

/// BlueZ sockaddr_rc — repr(C) gives 10 bytes with trailing padding,
/// matching the C compiler's layout that the kernel expects.
#[repr(C)]
struct SockaddrRc {
    rc_family: u16,
    rc_bdaddr: [u8; 6], // reversed byte order
    rc_channel: u8,
}

const _: () = assert!(std::mem::size_of::<SockaddrRc>() == 10);

/// Parse "XX:XX:XX:XX:XX:XX" → 6 bytes in BlueZ reversed order.
fn parse_mac(addr: &str) -> io::Result<[u8; 6]> {
    let parts: Vec<&str> = addr.split(':').collect();
    if parts.len() != 6 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid MAC address"));
    }
    let mut bytes = [0u8; 6];
    for (i, part) in parts.iter().rev().enumerate() {
        bytes[i] = u8::from_str_radix(part, 16)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid MAC hex"))?;
    }
    Ok(bytes)
}

pub struct RfcommSocket {
    fd: RawFd,
}

impl RfcommSocket {
    pub fn connect(mac: &str, channel: u8) -> io::Result<Self> {
        unsafe {
            let fd = libc::socket(AF_BLUETOOTH, libc::SOCK_STREAM, BTPROTO_RFCOMM);
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }

            // Set link-mode auth + encrypt
            let linkmode: u32 = RFCOMM_LM_AUTH | RFCOMM_LM_ENCRYPT;
            let ret = libc::setsockopt(
                fd,
                SOL_RFCOMM,
                RFCOMM_LM,
                &linkmode as *const u32 as *const libc::c_void,
                std::mem::size_of::<u32>() as libc::socklen_t,
            );
            if ret < 0 {
                libc::close(fd);
                return Err(io::Error::last_os_error());
            }

            let bdaddr = parse_mac(mac)?;
            let addr = SockaddrRc {
                rc_family: AF_BLUETOOTH as u16,
                rc_bdaddr: bdaddr,
                rc_channel: channel,
            };

            let ret = libc::connect(
                fd,
                &addr as *const SockaddrRc as *const libc::sockaddr,
                std::mem::size_of::<SockaddrRc>() as libc::socklen_t,
            );
            if ret < 0 {
                let err = io::Error::last_os_error();
                libc::close(fd);
                return Err(err);
            }

            Ok(Self { fd })
        }
    }

    pub fn set_timeout(&self, timeout: Duration) -> io::Result<()> {
        let tv = libc::timeval {
            tv_sec: timeout.as_secs() as libc::time_t,
            tv_usec: timeout.subsec_micros() as libc::suseconds_t,
        };
        unsafe {
            let ret = libc::setsockopt(
                self.fd,
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                &tv as *const libc::timeval as *const libc::c_void,
                std::mem::size_of::<libc::timeval>() as libc::socklen_t,
            );
            if ret < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
}

impl Read for RfcommSocket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = unsafe { libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }
}

impl Write for RfcommSocket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = unsafe { libc::write(self.fd, buf.as_ptr() as *const libc::c_void, buf.len()) };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl AsRawFd for RfcommSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl Drop for RfcommSocket {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}
