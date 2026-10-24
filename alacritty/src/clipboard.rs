#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
use std::error::Error;
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
use std::sync::Arc;
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
use std::time::Duration;

use log::{debug, warn};
use winit::raw_window_handle::RawDisplayHandle;

use alacritty_terminal::term::ClipboardType;

#[cfg(any(target_os = "macos", windows))]
use copypasta::ClipboardContext;
use copypasta::ClipboardProvider;
use copypasta::nop_clipboard::NopClipboardContext;
#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
use copypasta::wayland_clipboard;
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
use x11_clipboard::{Atom, Clipboard as X11Clipboard};

pub struct Clipboard {
    clipboard: Box<dyn ClipboardProvider>,
    selection: Option<Box<dyn ClipboardProvider>>,
}

impl Clipboard {
    pub unsafe fn new(display: RawDisplayHandle) -> Self {
        match display {
            #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
            RawDisplayHandle::Wayland(display) => {
                let (selection, clipboard) = unsafe {
                    wayland_clipboard::create_clipboards_from_external(display.display.as_ptr())
                };
                Self { clipboard: Box::new(clipboard), selection: Some(Box::new(selection)) }
            },
            _ => Self::default(),
        }
    }

    /// Used for tests, to handle missing clipboard provider when built without the `x11`
    /// feature, and as default clipboard value.
    pub fn new_nop() -> Self {
        Self { clipboard: Box::new(NopClipboardContext::new().unwrap()), selection: None }
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        #[cfg(any(target_os = "macos", windows))]
        return Self { clipboard: Box::new(ClipboardContext::new().unwrap()), selection: None };

        #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
        return {
            let (clipboard, selection) = X11Selection::new_pair().unwrap();
            Self { clipboard: Box::new(clipboard), selection: Some(Box::new(selection)) }
        };

        #[cfg(not(any(feature = "x11", target_os = "macos", windows)))]
        return Self::new_nop();
    }
}

impl Clipboard {
    pub fn store(&mut self, ty: ClipboardType, text: impl Into<String>) {
        let clipboard = match (ty, &mut self.selection) {
            (ClipboardType::Selection, Some(provider)) => provider,
            (ClipboardType::Selection, None) => return,
            _ => &mut self.clipboard,
        };

        clipboard.set_contents(text.into()).unwrap_or_else(|err| {
            warn!("Unable to store text in clipboard: {err}");
        });
    }

    pub fn load(&mut self, ty: ClipboardType) -> String {
        let clipboard = match (ty, &mut self.selection) {
            (ClipboardType::Selection, Some(provider)) => provider,
            _ => &mut self.clipboard,
        };

        match clipboard.get_contents() {
            Err(err) => {
                debug!("Unable to load text from clipboard: {err}");
                String::new()
            },
            Ok(text) => text,
        }
    }
}

/// Provider for a single X11 selection.
///
/// The `CLIPBOARD` and `PRIMARY` selections share one X11 clipboard, since every X11 clipboard
/// opens its own connections to the X server.
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
struct X11Selection {
    clipboard: Arc<X11Clipboard>,
    selection: Atom,
}

#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
impl X11Selection {
    /// Create providers for the `CLIPBOARD` and `PRIMARY` selections.
    fn new_pair() -> Result<(Self, Self), x11_clipboard::error::Error> {
        let clipboard = Arc::new(X11Clipboard::new()?);
        let atoms = &clipboard.getter.atoms;
        let (clipboard_atom, primary_atom) = (atoms.clipboard, atoms.primary);

        Ok((Self { clipboard: clipboard.clone(), selection: clipboard_atom }, Self {
            clipboard,
            selection: primary_atom,
        }))
    }
}

#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
impl ClipboardProvider for X11Selection {
    fn get_contents(&mut self) -> Result<String, Box<dyn Error + Send + Sync>> {
        let atoms = &self.clipboard.getter.atoms;
        let contents = self.clipboard.load(
            self.selection,
            atoms.utf8_string,
            atoms.property,
            Duration::from_secs(3),
        )?;
        Ok(String::from_utf8(contents)?)
    }

    fn set_contents(&mut self, contents: String) -> Result<(), Box<dyn Error + Send + Sync>> {
        let atoms = &self.clipboard.setter.atoms;
        Ok(self.clipboard.store(self.selection, atoms.utf8_string, contents)?)
    }
}
