// The standard Windows "Open" dialog, for choosing a file without dragging it.

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};

/// Blocks until the user picks a file or cancels. Run it off the main thread.
pub fn open_file() -> Option<String> {
    let mut buf = vec![0u16; 4096];
    let filter: Vec<u16> = "All files\0*.*\0Documents\0*.pdf;*.txt;*.md;*.doc;*.docx;*.csv;*.json\0Images\0*.png;*.jpg;*.jpeg;*.gif;*.webp\0\0"
        .encode_utf16()
        .collect();
    let title: Vec<u16> = "Choose a file for Mochi\0".encode_utf16().collect();

    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(buf.as_mut_ptr()),
        nMaxFile: buf.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        Flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    let ok = unsafe { GetOpenFileNameW(&mut ofn) };
    if !ok.as_bool() {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}
