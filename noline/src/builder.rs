//! Builder for editors

use core::marker::PhantomData;

use genawaiter::stack::Co;

use crate::{
    error::Error,
    history::{History, NoHistory, StaticHistory},
    line_buffer::{Buffer, NoBuffer, StaticBuffer},
    sync::{self, Read, Write, embedded::IO},
};

/// Builder for [`sync::Editor`] and [`no_sync::tokio::Editor`].
///
/// # Example
/// ```no_run
/// # use noline::sync::{Read, Write};
/// # struct IO {}
/// # impl Write for IO {
/// #     type Error = ();
/// #     fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error> { unimplemented!() }
/// #     fn flush(&mut self) -> Result<(), Self::Error> { unimplemented!() }
/// # }
/// # impl Read for IO {
/// #     type Error = ();
/// #     fn read(&mut self) -> Result<u8, Self::Error> { unimplemented!() }
/// # }
/// # let mut io = IO {};
/// use noline::builder::EditorBuilder;
///
/// let mut editor = EditorBuilder::new_static::<100>()
///     .with_static_history::<200>()
///     .build_sync(&mut io)
///     .unwrap();
/// ```
pub struct EditorBuilder<B: Buffer, H: History> {
    _marker: PhantomData<(B, H)>,
}

impl EditorBuilder<NoBuffer, NoHistory> {
    /// Create builder for editor with static buffer
    ///
    /// # Example
    /// ```
    /// use noline::builder::EditorBuilder;
    ///
    /// let builder = EditorBuilder::new_static::<100>();
    /// ```
    pub fn new_static<const N: usize>() -> EditorBuilder<StaticBuffer<N>, NoHistory> {
        EditorBuilder {
            _marker: PhantomData,
        }
    }
}

impl<B: Buffer, H: History> EditorBuilder<B, H> {
    /// Add static history
    pub fn with_static_history<const N: usize>(self) -> EditorBuilder<B, StaticHistory<N>> {
        EditorBuilder {
            _marker: PhantomData,
        }
    }

    /// Build [`sync::Editor`]. Is equivalent of calling [`sync::Editor::new()`].
    pub async fn build_sync<RW, RE, WE>(
        self,
        co: &mut Co<'_, ()>,
        io: &mut IO<RW>,
    ) -> Result<sync::Editor<B, H, RW>, Error<RE, WE>>
    where
        RW: embedded_hal::serial::Read<u8, Error = RE> + embedded_hal::serial::Write<u8, Error = WE>,
    {
        sync::Editor::new(co, io).await
    }
}
