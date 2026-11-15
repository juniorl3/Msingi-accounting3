//! Sends raw ESC/POS bytes to a receipt printer. Standard library only.
//!
//! Two ways to reach a printer:
//! * `network`: straight to the printer's IP address (normally port 9100). No drivers needed.
//! * `queue`:   to a print queue installed on this computer (for USB printers). Mac/Linux: `lp -o raw`. Windows: the print spooler in RAW mode.
//!
//! The same bytes carry the cash-drawer pulse, because the drawer is wired to the printer.

use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs};
use std::process::{Command, Stdio};
use std::time::Duration;

/// A receipt is a few kilobytes; refuse anything absurd.
pub const MAX_BYTES: usize = 256 * 1024;

/// A host name or IP address: letters, digits, dot, dash, underscore, colon (IPv6). No spaces or anything a shell or URL could misread.
pub fn valid_host(h: &str) -> bool {
    !h.is_empty() && h.len() <= 253 && h.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':'))
}

/// A print-queue name as CUPS allows it (no spaces or slashes), so it can never be read as an option.
pub fn valid_queue(q: &str) -> bool {
    !q.is_empty() && q.len() <= 127 && !q.starts_with('-') && q.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// A Windows printer name: spaces, brackets and a leading `\\server\` are normal there. Control characters are not.
pub fn valid_win_queue(q: &str) -> bool {
    !q.trim().is_empty() && q.chars().count() <= 127 && !q.chars().any(|c| c.is_control())
}

fn check_size(data: &[u8]) -> Result<(), String> {
    if data.is_empty() {
        return Err("Nothing to print.".into());
    }
    if data.len() > MAX_BYTES {
        return Err("The print job is too large.".into());
    }
    Ok(())
}

/// Sends `data` to host:port over TCP. Fails with a readable message if the printer cannot be reached within `timeout_ms`.
pub fn send_network(host: &str, port: u16, data: &[u8], timeout_ms: u64) -> Result<(), String> {
    check_size(data)?;
    if !valid_host(host) {
        return Err("Enter the printer's IP address or name, for example 192.168.1.50.".into());
    }
    if port == 0 {
        return Err("Enter the printer's port (usually 9100).".into());
    }
    let timeout = Duration::from_millis(timeout_ms.clamp(500, 30_000));
    let addrs: Vec<_> = (host, port)
        .to_socket_addrs()
        .map_err(|_| format!("Could not find a printer called {host}. Check the address."))?
        .collect();
    let mut last = String::from("no address");
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(mut stream) => {
                stream.set_write_timeout(Some(timeout)).map_err(|e| e.to_string())?;
                stream.write_all(data).map_err(|e| format!("Sending to the printer failed: {e}"))?;
                stream.flush().map_err(|e| format!("Sending to the printer failed: {e}"))?;
                // Give slow printers a moment to take the last bytes before the connection closes.
                std::thread::sleep(Duration::from_millis(150));
                let _ = stream.shutdown(std::net::Shutdown::Both);
                return Ok(());
            }
            Err(e) => last = e.to_string(),
        }
    }
    Err(format!("Could not reach the printer at {host}:{port} ({last}). Check it is on, connected to the network, and the address is right."))
}

/// Sends `data` to a print queue with `lp -d <queue> -o raw` (macOS and Linux).
#[cfg(unix)]
pub fn send_queue(queue: &str, data: &[u8]) -> Result<(), String> {
    check_size(data)?;
    if !valid_queue(queue) {
        return Err("Choose the printer's print queue.".into());
    }
    let mut child = Command::new("lp")
        .args(["-d", queue, "-o", "raw"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not start the system print command: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("Could not open the print command.")?
        .write_all(data)
        .map_err(|e| format!("Sending to the print queue failed: {e}"))?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(format!("The print queue {queue} refused the job{}", if why.is_empty() { ".".to_string() } else { format!(": {why}") }))
    }
}

#[cfg(windows)]
pub fn send_queue(queue: &str, data: &[u8]) -> Result<(), String> {
    check_size(data)?;
    if !valid_win_queue(queue) {
        return Err("Choose the printer's print queue.".into());
    }
    win::send_raw(queue, data)
}

#[cfg(all(not(unix), not(windows)))]
pub fn send_queue(_queue: &str, _data: &[u8]) -> Result<(), String> {
    Err("Printing through a system print queue is not available on this system. Use a network printer.".into())
}

/// Parses `lpstat -e` (one destination per line), keeping only names that are safe to pass to `lp`.
pub fn parse_queues(out: &str) -> Vec<String> {
    let mut v: Vec<String> = out.lines().map(|l| l.trim().to_string()).filter(|l| valid_queue(l)).collect();
    v.sort();
    v.dedup();
    v
}

#[cfg(unix)]
pub fn list_queues() -> Result<Vec<String>, String> {
    let out = Command::new("lpstat").arg("-e").output().map_err(|e| format!("Could not list printers: {e}"))?;
    Ok(parse_queues(&String::from_utf8_lossy(&out.stdout)))
}

#[cfg(windows)]
pub fn list_queues() -> Result<Vec<String>, String> {
    let mut v: Vec<String> = win::list()?.into_iter().filter(|n| valid_win_queue(n)).collect();
    v.sort();
    v.dedup();
    Ok(v)
}

#[cfg(all(not(unix), not(windows)))]
pub fn list_queues() -> Result<Vec<String>, String> {
    Ok(Vec::new())
}

/// Windows print spooler, called directly (winspool.drv ships with Windows). Sends bytes untouched with the RAW data type,
/// so ESC/POS commands reach the printer exactly as with a network printer. Not testable off Windows.
#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::ptr::null;

    #[repr(C)]
    struct DocInfo1W { doc_name: *const u16, output_file: *const u16, datatype: *const u16 }
    #[repr(C)]
    struct PrinterInfo4W { name: *const u16, server: *const u16, attributes: u32 }

    #[link(name = "winspool")]
    extern "system" {
        fn OpenPrinterW(name: *const u16, handle: *mut *mut c_void, defaults: *const c_void) -> i32;
        fn StartDocPrinterW(handle: *mut c_void, level: u32, info: *const DocInfo1W) -> u32;
        fn WritePrinter(handle: *mut c_void, buf: *const c_void, len: u32, written: *mut u32) -> i32;
        fn EndDocPrinter(handle: *mut c_void) -> i32;
        fn ClosePrinter(handle: *mut c_void) -> i32;
        fn EnumPrintersW(flags: u32, name: *const u16, level: u32, buf: *mut u8, size: u32, needed: *mut u32, returned: *mut u32) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }

    unsafe fn read_wide(p: *const u16) -> String {
        if p.is_null() { return String::new(); }
        let mut n = 0;
        while *p.add(n) != 0 { n += 1; }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
    }

    pub fn send_raw(queue: &str, data: &[u8]) -> Result<(), String> {
        unsafe {
            let name = wide(queue);
            let mut h: *mut c_void = std::ptr::null_mut();
            if OpenPrinterW(name.as_ptr(), &mut h, null()) == 0 {
                return Err(format!("Could not open the printer \"{queue}\". Check it is installed, switched on and connected."));
            }
            let doc = wide("Msingi receipt"); let kind = wide("RAW");
            let info = DocInfo1W { doc_name: doc.as_ptr(), output_file: null(), datatype: kind.as_ptr() };
            if StartDocPrinterW(h, 1, &info) == 0 {
                ClosePrinter(h);
                return Err(format!("The printer \"{queue}\" would not accept a print job."));
            }
            let mut sent = 0usize;
            let mut failed = false;
            while sent < data.len() {
                let mut written: u32 = 0;
                let chunk = (data.len() - sent).min(16 * 1024) as u32;
                if WritePrinter(h, data.as_ptr().add(sent) as *const c_void, chunk, &mut written) == 0 || written == 0 { failed = true; break; }
                sent += written as usize;
            }
            EndDocPrinter(h);
            ClosePrinter(h);
            if failed { return Err(format!("Sending to the printer \"{queue}\" failed part way. Check the printer and try again.")); }
            Ok(())
        }
    }

    pub fn list() -> Result<Vec<String>, String> {
        const FLAGS: u32 = 0x2 | 0x4; // local printers and printers connected from a network share
        unsafe {
            let (mut needed, mut returned) = (0u32, 0u32);
            EnumPrintersW(FLAGS, null(), 4, std::ptr::null_mut(), 0, &mut needed, &mut returned);
            if needed == 0 { return Ok(Vec::new()); }
            let mut buf = vec![0u64; (needed as usize + 7) / 8]; // 8-byte aligned
            if EnumPrintersW(FLAGS, null(), 4, buf.as_mut_ptr() as *mut u8, needed, &mut needed, &mut returned) == 0 {
                return Err("Could not list the printers on this computer.".into());
            }
            let items = buf.as_ptr() as *const PrinterInfo4W;
            Ok((0..returned as usize).map(|i| read_wide((*items.add(i)).name)).filter(|n| !n.is_empty()).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;

    #[test]
    fn validates_hosts_and_queues() {
        for ok in ["192.168.1.50", "printer.local", "my-printer_1", "fe80::1"] {
            assert!(valid_host(ok), "{ok}");
        }
        for bad in ["", "a b", "x;rm", "http://x", "a/b", "$(x)"] {
            assert!(!valid_host(bad), "{bad}");
        }
        assert!(valid_queue("EPSON_TM_T20III") && valid_queue("Printer-1.local"));
        for bad in ["", "-d", "a b", "a/b", "a;b", "--help"] {
            assert!(!valid_queue(bad), "{bad}");
        }
    }

    #[test]
    fn windows_printer_names_allow_spaces_but_not_control_characters() {
        for ok in ["EPSON TM-T20II Receipt", "\\\\SHOP-PC\\XP-80C", "POS-80 (Copy 1)"] {
            assert!(valid_win_queue(ok), "{ok}");
        }
        for bad in ["", "   ", "a\nb", "a\0b"] {
            assert!(!valid_win_queue(bad), "{bad:?}");
        }
        assert!(!valid_win_queue(&"x".repeat(128)));
    }

    #[test]
    fn rejects_empty_and_oversized_jobs() {
        assert!(send_network("127.0.0.1", 9100, &[], 1000).is_err());
        assert!(send_network("127.0.0.1", 9100, &vec![0u8; MAX_BYTES + 1], 1000).is_err());
        assert!(send_network("bad host", 9100, &[1], 1000).unwrap_err().contains("IP address"));
        assert!(send_network("127.0.0.1", 0, &[1], 1000).unwrap_err().contains("port"));
    }

    #[test]
    fn delivers_every_byte_over_tcp() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let reader = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut got = Vec::new();
            s.read_to_end(&mut got).unwrap();
            got
        });
        // includes bytes that are not valid text: ESC @, drawer pulse, cut
        let mut data = vec![0x1b, 0x40, 0x1b, 0x70, 0x00, 25, 250];
        data.extend((0..=255u8).cycle().take(5000));
        data.extend([0x1d, 0x56, 0x42, 3]);
        send_network("127.0.0.1", port, &data, 2000).unwrap();
        assert_eq!(reader.join().unwrap(), data);
    }

    #[test]
    fn reports_an_unreachable_printer_clearly() {
        let port = { let l = TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() }; // closed again
        let e = send_network("127.0.0.1", port, &[1, 2, 3], 1000).unwrap_err();
        assert!(e.contains("Could not reach the printer"), "{e}");
    }

    #[test]
    fn parses_lpstat_output() {
        let q = parse_queues("EPSON_TM_T20III\n  Brother HL \nHP-LaserJet\nEPSON_TM_T20III\n-bad\n\n");
        assert_eq!(q, vec!["EPSON_TM_T20III".to_string(), "HP-LaserJet".to_string()]);
    }

    /// Uses fake `lp` and `lpstat` scripts first on PATH. This is the only test that touches PATH.
    #[cfg(unix)]
    #[test]
    fn sends_to_a_queue_with_lp_raw() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("msingi-lp-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (args, body) = (dir.join("args.txt"), dir.join("body.bin"));
        let script = |name: &str, text: String| {
            let p = dir.join(name);
            std::fs::write(&p, text).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        script("lp", format!("#!/bin/sh\necho \"$@\" > {}\ncat > {}\n", args.display(), body.display()));
        script("lpstat", "#!/bin/sh\nprintf 'ReceiptPrinter\\nOffice-Laser\\n'\n".to_string());
        let old = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{old}", dir.display()));

        let data = vec![0x1b, 0x40, 0xff, 0x00, b'H', b'i', 0x1d, 0x56, 0x42, 3];
        send_queue("ReceiptPrinter", &data).unwrap();
        assert_eq!(std::fs::read_to_string(&args).unwrap().trim(), "-d ReceiptPrinter -o raw");
        assert_eq!(std::fs::read(&body).unwrap(), data);
        assert!(send_queue("bad queue", &data).is_err());
        assert_eq!(list_queues().unwrap(), vec!["Office-Laser".to_string(), "ReceiptPrinter".to_string()]);

        script("lp", "#!/bin/sh\ncat > /dev/null\necho 'lp: Unknown destination' >&2\nexit 1\n".to_string());
        let e = send_queue("ReceiptPrinter", &data).unwrap_err();
        assert!(e.contains("refused the job") && e.contains("Unknown destination"), "{e}");

        std::env::set_var("PATH", old);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
