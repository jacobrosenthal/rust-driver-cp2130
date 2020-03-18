//! CP2130 Driver
//! 
//! 
//! Copyright 2019 Ryan Kurte

use std::sync::{Arc, Mutex};

#[macro_use]
extern crate log;

#[macro_use]
extern crate bitflags;
extern crate byteorder;

#[macro_use]
extern crate lazy_static;

extern crate embedded_hal;
pub use embedded_hal::spi::{Mode as SpiMode};

extern crate rusb;
use rusb::{Device as UsbDevice, Context, DeviceDescriptor};

pub mod device;
pub use crate::device::{GpioMode, GpioLevel, SpiConfig, SpiClock};

use crate::device::*;

pub mod manager;

pub mod prelude;

#[derive(Debug)]
pub enum Error {
//    Io(IoError),
    Usb(rusb::Error),
    NoLanguages,
    Configurations,
    Endpoint,
    GpioInUse,
    InvalidIndex,
    InvalidBaud,
}

impl From<rusb::Error> for Error {
    fn from(e: rusb::Error) -> Self {
        Error::Usb(e)
    }
}

/// CP2130 provides methods to interact with the device, as well as create new spi and gpio connectors.
pub struct Cp2130 {
    inner: Arc<Mutex<Inner>>,
    info: Info,
}

/// Device trait provides methods directly on the CP2130
pub trait Device {
    /// Read from the SPI device
    fn spi_read(&self, buff: &mut [u8]) -> Result<usize, Error>;
    
    /// Write to the SPI device
    fn spi_write(&self, buff: &[u8]) -> Result<(), Error>;

    // Transfer (write-read) to and from the SPI device
    fn spi_write_read(&self, buff_out: &[u8], buff_in: &mut [u8]) -> Result<usize, Error>;
    
    /// Fetch the CP2130 chip version
    fn version(&self) -> Result<u16, Error> ;

    /// Set the mode and level for a given GPIO pin
    fn set_gpio_mode_level(&self, pin: u8, mode: GpioMode, level: GpioLevel) -> Result<(), Error>;
    
    /// Fetch the values for all GPIO pins
    fn get_gpio_values(&self) -> Result<GpioLevels, Error>;
    
    /// Fetch the value for a given GPIO pin
    fn get_gpio_level(&self, pin: u8) -> Result<bool, Error>;
}

impl Cp2130{
    /// Create a new CP2130 instance from a libusb device and descriptor
    pub fn new(device: UsbDevice<Context>, descriptor: DeviceDescriptor) -> Result<Self, Error> {
        
        // Connect to device
        let (inner, info) = Inner::new(device, descriptor)?;
        let inner = Arc::new(Mutex::new(inner));

        // Create wrapper object
        Ok(Self{info, inner})
    }

    /// Fetch information for the connected device
    pub fn info(&self) -> Info {
        self.info.clone()
    }

    pub fn reset(&self) -> Result<(), Error> {
        self.inner.lock().unwrap().reset()
    }

    /// Create an SPI connector
    pub fn spi(&self, channel: u8, config: SpiConfig) -> Result<Spi, Error> {
        let mut inner = self.inner.lock().unwrap();

        // Configure SPI
        inner.spi_configure(channel, config)?;

        Ok(Spi{inner: self.inner.clone(), _channel: channel})
    }

    /// Create a GPIO OutputPin
    pub fn gpio_out(&self, index: u8, mode: GpioMode, level: GpioLevel) -> Result<OutputPin, Error> {
        let mut inner = self.inner.lock().unwrap();

        if inner.gpio_allocated[index as usize] {
            return Err(Error::GpioInUse)
        }

        inner.set_gpio_mode_level(index, mode, level)?;
        inner.gpio_allocated[index as usize] = true;

        Ok(OutputPin{index, mode, inner: self.inner.clone()})
    }

    /// Create a GPIO InputPin
    pub fn gpio_in(&self, index: u8) -> Result<InputPin, Error> {
        let mut inner = self.inner.lock().unwrap();

        if inner.gpio_allocated[index as usize] {
            return Err(Error::GpioInUse)
        }

        inner.set_gpio_mode_level(index, GpioMode::Input, GpioLevel::Low)?;
        inner.gpio_allocated[index as usize] = true;

        Ok(InputPin{index, inner: self.inner.clone()})
    }

}

/// Underlying device functions
impl Device for Cp2130{
    fn spi_read(&self, buff: &mut [u8]) -> Result<usize, Error> {
        let mut inner = self.inner.lock().unwrap();
        inner.spi_read(buff)
    }

    fn spi_write(&self, buff: &[u8]) -> Result<(), Error> {
        let mut inner = self.inner.lock().unwrap();
        inner.spi_write(buff)
    }

    fn spi_write_read(&self, buff_out: &[u8], buff_in: &mut [u8]) -> Result<usize, Error> {
        let mut inner = self.inner.lock().unwrap();
        inner.spi_write_read(buff_out, buff_in)
    }

    fn version(&self) -> Result<u16, Error>  {
        let mut inner = self.inner.lock().unwrap();
        inner.version()
    }

    fn set_gpio_mode_level(&self, pin: u8, mode: GpioMode, level: GpioLevel) -> Result<(), Error> {
        let mut inner = self.inner.lock().unwrap();
        inner.set_gpio_mode_level(pin, mode, level)
    }

    fn get_gpio_values(&self) -> Result<GpioLevels, Error> {
        let mut inner = self.inner.lock().unwrap();
        inner.get_gpio_values()
    }

    fn get_gpio_level(&self, pin: u8) -> Result<bool, Error> {
        let mut inner = self.inner.lock().unwrap();
        inner.get_gpio_level(pin)
    }
}

/// Spi object implements embedded-hal SPI traits for the CP2130
pub struct Spi{
    // TODO: use channel configuration
    _channel: u8,
    inner: Arc<Mutex<Inner>>,
}

use embedded_hal::blocking::spi::{Write, Transfer, Transactional, Operation};

impl Transfer<u8> for Spi {
    type Error = Error;

    fn transfer<'w>(&mut self, words: &'w mut [u8] ) -> Result<&'w [u8], Self::Error> {
        let out = words.to_vec();
        let _n = self.inner.lock().unwrap().spi_write_read(&out, words)?;
        Ok(words)
    }
}

impl Write<u8> for Spi {
    type Error = Error;

    fn write(&mut self, words: &[u8] ) -> Result<(), Self::Error> {
        let _n = self.inner.lock().unwrap().spi_write(words)?;
        Ok(())
    }
}

impl Transactional<u8> for Spi {
    type Error = Error;

    fn exec<'b>(&mut self, operations: &mut [Operation<'b, u8>]) -> Result<(), Self::Error> {

        for o in operations.iter_mut() {
            match o {
                Operation::Write(d) => self.write(d)?,
                Operation::Transfer(d) => self.transfer(d).map(|_| ())?,
            }
        }

        Ok(())
    }
}


/// InputPin object implements embedded-hal InputPin traits for the CP2130
pub struct InputPin{
    index: u8,
    inner: Arc<Mutex<Inner>>,
}

impl embedded_hal::digital::v2::InputPin for InputPin{
    type Error = Error;

    fn is_high(&self) -> Result<bool, Self::Error> {
        self.inner.lock().unwrap().get_gpio_level(self.index)
    }

    fn is_low(&self) -> Result<bool, Self::Error> {
        let v = self.is_high()?;
        Ok(!v)
    }
}

/// OutputPin object implements embedded-hal OutputPin traits for the CP2130
pub struct OutputPin{
    index: u8,
    mode: GpioMode,
    inner: Arc<Mutex<Inner>>,
}

impl embedded_hal::digital::v2::OutputPin for OutputPin{
    type Error = Error;

    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.inner.lock().unwrap().set_gpio_mode_level(self.index, self.mode, GpioLevel::High)
    }

    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.inner.lock().unwrap().set_gpio_mode_level(self.index, self.mode, GpioLevel::Low)
    }
}
