#![no_main]
#![no_std]
#![feature(ascii_char)]

#[allow(unused_imports)]
use defmt_rtt as _;
#[allow(unused_imports)]
use panic_probe as _;

use core::pin::Pin;
use cortex_m_rt::entry;
use stm32f7xx_hal as hal;
use stm32f7xx_hal::{pac, prelude::*};

use defmt::info;
use embedded_graphics::image::{Image, ImageRaw};
use embedded_graphics::mono_font::ascii::FONT_9X15;
use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use embedded_graphics::Drawable;
use hal::serial::Config;
use ssd1306::rotation::DisplayRotation;
use ssd1306::size::DisplaySize128x32;
use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306};
use stm32f7xx_hal::dma::DMA;

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();

    let mut rcc = dp.RCC.constrain();

    let dma = DMA::new(dp.DMA1);

    let mut rx_stream = dma.streams.stream2;

    let dma = dma.handle.enable(&mut rcc.ahb1);

    let clocks = rcc.cfgr.sysclk(216_000_000.Hz()).pclk1(13.MHz()).freeze();

    info!("hello");

    let gpiod = dp.GPIOD.split();
    let rx = gpiod.pd0.into_alternate();
    let tx = gpiod.pd1.into_alternate();

    let uart_config = Config {
        baud_rate: 115200.bps(),
        ..Default::default()
    };
    let uart = hal::serial::Serial::new(dp.UART4, (tx, rx), &clocks, uart_config);

    let (mut tx, mut rx) = uart.split();

    let gpiob = dp.GPIOB.split();

    let scl = gpiob.pb8.into_alternate_open_drain();
    let sda = gpiob.pb9.into_alternate_open_drain();

    let i2c = hal::i2c::BlockingI2c::i2c1(
        dp.I2C1,
        (scl, sda),
        hal::i2c::Mode::FastPlus {
            frequency: 400_000.Hz(),
        },
        &clocks,
        &mut rcc.apb1,
        50_000,
    );

    let interface = I2CDisplayInterface::new(i2c);
    let mut display = Ssd1306::new(interface, DisplaySize128x32, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();
    display.init().unwrap();

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_9X15)
        .text_color(BinaryColor::On)
        .build();

    display.clear(BinaryColor::Off).unwrap();
    Text::with_baseline(
        "bad apple\n waiting input",
        Point::zero(),
        text_style,
        Baseline::Top,
    )
    .draw(&mut display)
    .unwrap();
    display.flush().unwrap();

    // 48*32
    static mut BUFFER: [u8; 192] = [0u8; 192];

    // 128*32
    // static mut BUFFER: [u8; 512 * 16] = [0u8; 512 * 16];
    let mut buffer = unsafe { Pin::new(&mut BUFFER) };

    loop {
        tx.write(0b11001100).unwrap();
        let mut transfer = rx.read_all(buffer, &dma, rx_stream);
        let res = cortex_m::interrupt::free(|_| {
            transfer.enable_interrupts(
                &dma,
                stm32f7xx_hal::dma::Interrupts {
                    transfer_complete: true,
                    transfer_error: true,
                    direct_mode_error: true,
                    ..stm32f7xx_hal::dma::Interrupts::default()
                },
            );

            let transfer = transfer.start(&dma);

            cortex_m::asm::wfi();

            transfer.wait(&dma).unwrap()
        });
        buffer = res.buffer;
        rx = res.target;
        rx_stream = res.stream;

        let raw_image = ImageRaw::<BinaryColor>::new(buffer.as_slice(), 48);

        let image = Image::new(&raw_image, Point::zero());
        display.clear(BinaryColor::Off).unwrap();
        image.draw(&mut display).unwrap();
        display.flush().unwrap();
    }
}
