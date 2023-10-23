https://chacin.dev/blog/cross-compiling-rust-for-the-raspberry-pi/
https://chacin.dev/blog/cross-compiling-rust-for-the-raspberry-pi/
https://github.com/robamu-org/rpi-rs-crosscompile/blob/main/README.md#prerequisites
https://chacin.dev/blog/cross-compiling-rust-for-the-raspberry-pi/
https://medium.com/swlh/compiling-rust-for-raspberry-pi-arm-922b55dbb050
https://gitlab.com/alelec/arm-none-eabi-gcc-deb/-/releases

rustup target add armv7-unknown-linux-gnueabi

sudo apt install gcc-arm-linux-gnueabi
sudo apt install gcc make gcc-arm-linux-gnueabi binutils-arm-linux-gnueabi

rustup target add armv7-unknown-linux-gnueabi

sudo systemctl daemon-reload
sudo systemctl enable SpeedSkating-Backend.service

sudo systemctl start SpeedSkating-Backend.service


cross build --release --target=armv7-unknown-linux-gnueabihf

sudo apt-get install gcc-arm-linux-gnueabihf

scp SpeedSkating-Backend.service pi@raspberrypi.local:/lib/systemd/system/
systemctl daemon-reload
systemctl start SpeedSkating-Backend.service
systemctl enable SpeedSkating-Backend.service