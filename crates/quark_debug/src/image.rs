//! Extensions/traits on the `Lepton3` image in rust for more
//! debugging support

use lepton3::{format::Image, lepton_image::flags::ImageFlags};

/// A trait that allows us to strip debug information from the Lepton3
/// image
pub trait DebugStrippableImage {
    fn strip_debug(&mut self);
}

impl DebugStrippableImage for Image {
    fn strip_debug(&mut self) {
        self.debug_info = None;
        self.header.flags.clear(ImageFlags::DEBUG_INFO);
    }
}
