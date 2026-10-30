//! FS server (hgfs) entry point (placeholder).

fn main() {
    // Not yet a real service process (event loop + RS boot protocol).
    minix_fs_hgfs::init();
    loop {}
}
