//! Line editor for synchronous IO.
//!
//! The editor takes a struct implementing the [`Read`] and [`Write`]
//! traits. There are ready made implementations in [`std::IO`] and [`embedded::IO`].
//!
//! Use the [`crate::builder::EditorBuilder`] to build an editor.
use ::core::marker::PhantomData;

use embedded_hal::serial;
use genawaiter::stack::Co;

use crate::error::Error;
use crate::history::{get_history_entries, CircularSlice, History};
use crate::line_buffer::{Buffer, LineBuffer};

use crate::core::{Initializer, InitializerResult, Line};
use crate::output::{Output, OutputItem};
use crate::terminal::Terminal;

/// Trait for reading bytes from input
pub trait Read {
    type Error;

    /// Read single byte from input
    fn read(&mut self) -> Result<u8, Self::Error>;
}

/// Trait for writing bytes to output
pub trait Write {
    type Error;

    /// Write byte slice to output
    fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error>;

    /// Flush output
    fn flush(&mut self) -> Result<(), Self::Error>;
}

/// Line editor for synchronous IO
///
/// It is recommended to use [`crate::builder::EditorBuilder`] to build an Editor.
pub struct Editor<B: Buffer, H: History, RW: serial::Read<u8> + serial::Write<u8>> {
    buffer: LineBuffer<B>,
    terminal: Terminal,
    history: H,
    _marker: PhantomData<RW>,
}

impl<B, H, RW, RE, WE> Editor<B, H, RW>
where
    B: Buffer,
    H: History,
    RW: serial::Read<u8, Error = RE> + serial::Write<u8, Error = WE>,
{
    /// Create and initialize line editor
    pub async fn new(
        co: &mut Co<'_, ()>,
        io: &mut embedded::IO<RW>,
    ) -> Result<Self, Error<RE, WE>> {
        let mut initializer = Initializer::new();

        io.write(co, Initializer::init())
            .await
            .or_else(|err| Error::write_error(err))?;
        io.flush(co).await.or_else(|err| Error::write_error(err))?;

        let terminal = loop {
            let byte = io.read(co).await.or_else(|err| Error::read_error(err))?;

            match initializer.advance(byte) {
                InitializerResult::Continue => (),
                InitializerResult::Item(terminal) => break terminal,
                InitializerResult::InvalidInput => return Err(Error::ParserError),
            }
        };

        Ok(Self {
            buffer: LineBuffer::new(),
            terminal,
            history: H::default(),
            _marker: PhantomData,
        })
    }

    async fn handle_output<'b>(
        co: &mut Co<'_, ()>,
        output: Output<'b, B>,
        io: &mut embedded::IO<RW>,
    ) -> Result<Option<()>, Error<RE, WE>> {
        for item in output {
            if let Some(bytes) = item.get_bytes() {
                io.write(co, bytes)
                    .await
                    .or_else(|err| Error::write_error(err))?;
            }

            io.flush(co).await.or_else(|err| Error::write_error(err))?;

            match item {
                OutputItem::EndOfString => return Ok(Some(())),
                OutputItem::Abort => return Err(Error::Aborted),
                _ => (),
            }
        }

        Ok(None)
    }

    /// Read line from `stdin`
    pub async fn readline<'b>(
        &'b mut self,
        co: &mut Co<'_, ()>,
        prompt: &'b str,
        io: &mut embedded::IO<RW>,
    ) -> Result<&'b str, Error<RE, WE>> {
        let mut line = Line::new(
            prompt,
            &mut self.buffer,
            &mut self.terminal,
            &mut self.history,
        );
        Self::handle_output(co, line.reset(), io).await?;

        loop {
            let byte = io.read(co).await.or_else(|err| Error::read_error(err))?;
            if Self::handle_output(co, line.advance(byte), io)
                .await?
                .is_some()
            {
                break;
            }
        }

        Ok(self.buffer.as_str())
    }

    /// Load history from iterator
    pub fn load_history<'a>(&mut self, entries: impl Iterator<Item = &'a str>) -> usize {
        self.history.load_entries(entries)
    }

    /// Get history as iterator over circular slices
    pub fn get_history<'a>(&'a self) -> impl Iterator<Item = CircularSlice<'a>> {
        get_history_entries(&self.history)
    }
}

pub mod embedded {
    //! IO implementation using traits from [`embedded_hal::serial`]. Requires feature `embedded`.

    use core::fmt;

    use embedded_hal::serial;

    macro_rules! ablock {
        ($co:expr, $e:expr) => {
            loop {
                match $e {
                    Ok(v) => break Ok(v),
                    Err(::nb::Error::WouldBlock) => $co.yield_(()).await,
                    Err(::nb::Error::Other(e)) => break Err(e),
                }
            }
        };
    }

    use super::*;

    /// IO wrapper for [`embedded_hal::serial::Read`] and [`embedded_hal::serial::Write`]
    pub struct IO<RW>
    where
        RW: serial::Read<u8> + serial::Write<u8>,
    {
        rw: RW,
    }

    impl<RW> IO<RW>
    where
        RW: serial::Read<u8> + serial::Write<u8>,
    {
        pub fn new(rw: RW) -> Self {
            Self { rw }
        }

        /// Consume self and return wrapped object
        pub fn take(self) -> RW {
            self.rw
        }

        /// Return mutable reference to wrapped object
        pub fn inner(&mut self) -> &mut RW {
            &mut self.rw
        }

        pub async fn read(
            &mut self,
            co: &mut Co<'_, ()>,
        ) -> Result<u8, <RW as serial::Read<u8>>::Error> {
            ablock!(co, self.rw.read())
        }

        pub async fn write(
            &mut self,
            co: &mut Co<'_, ()>,
            buf: &[u8],
        ) -> Result<(), <RW as serial::Write<u8>>::Error> {
            for &b in buf {
                ablock!(co, self.rw.write(b))?;
            }

            Ok(())
        }

        pub async fn flush(
            &mut self,
            co: &mut Co<'_, ()>,
        ) -> Result<(), <RW as serial::Write<u8>>::Error> {
            ablock!(co, self.rw.flush())
        }
    }

    // impl<RW> fmt::Write for IO<RW>
    // where
    //     RW: serial::Read<u8> + serial::Write<u8>,
    // {
    //     fn write_str(&mut self, s: &str) -> fmt::Result {
    //         self.write(s.as_bytes()).or(Err(fmt::Error))
    //     }
    // }
}
