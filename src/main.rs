use serialport;
use serialport::{DataBits, FlowControl, Parity, SerialPort, StopBits, TTYPort};

use serde::{Deserialize, Serialize};

use std::time::Duration;
use std::{thread};
use std::io::{Read, Write};
use std::io::ErrorKind::TimedOut;

#[derive(Serialize, Deserialize, Debug)]
struct Frame
{
    header : [u8; 3],
    size : u16,
    payload : Vec<u8>,
    checksum : u8,
}

fn new<'a>(payload: Vec<u8>) -> Frame {
    let mut frame = Frame {
        header : [0x5a, 0x77, 0xff],
        size : payload.len() as u16,
        payload : payload.into(),
        checksum : 0,
    };
    frame.checksum = frame.calculate_checksum().unwrap();
    frame
}


impl Frame
{

fn calculate_checksum(&self) -> Result<u8, ()>
{
    let mut sum = 0;
    let mut bytes = match bincode::serialize(&self)
    {
        Ok(bytes) => bytes,
        Err(msg) =>
        {
            println!("Failed to serialize {}", msg);
            return Err(())
        }
    };
    bytes.drain(5..13);
    let interesting_bytes = &bytes[3..bytes.len()-1]; // exclude header and checksum
    for byte in interesting_bytes
    {
        sum ^= byte;
    }
    Ok(sum)
}

fn as_bytes(&self) -> Result<Vec<u8>, ()>
{
    let mut bytes = match bincode::serialize(&self)
    {
        Ok(bytes) => bytes,
        Err(msg) =>
            {
                println!("Failed to serialize {}", msg);
                return Err(())
            }
    };
    bytes.drain(5..13);
    Ok(bytes)
}

}

fn read_frame(serial_port : &mut TTYPort, payload_size : u32) -> Result<Frame, ()>
{
    let mut frame = vec![0u8; (payload_size + 6) as usize];

    loop
    {
        serial_port.set_timeout(Duration::from_millis(130)).expect("Couldn't set a tiemout");
        match serial_port.read(&mut frame)
        {
            Ok(_) =>
            {
                let payload = frame.drain(5..(payload_size+5) as usize).collect();
                return Ok(new(payload));
            },
            Err(msg) =>
            {
                if msg.kind() == TimedOut
                {
                    println!("Timed out reading from serial!");
                    continue;
                }
                else
                {
                    println!("Error reading device info from serial!, {}", msg);
                    return Err(());
                }
            },
        }
    }

}

fn main()
{
    let serial_port_builder = serialport::new("/dev/ttyUSB0", 3000000)
    .data_bits(DataBits::Eight)
    .parity(Parity::None)
    .stop_bits(StopBits::One)
    .flow_control(FlowControl::None)
        ;
    let mut serial_port = match serial_port_builder.open_native()
    {
        Ok(port) => { port },
        Err(msg) => { println!("Error opening port!, {}", msg); return ; },
    };

    println!("Opened serial port with baud {:?}", serial_port.baud_rate());

    let baud_rate = new(vec![0x12, 0x55]);
    match serial_port.write(&baud_rate.as_bytes().unwrap())
    {
        Ok(_) => {  },
        Err(msg) => { println!("Error writing baud info!, {}", msg); return ; },
    };

    let device_info = new(vec![0x10, 0x00]);
    match serial_port.write(&device_info.as_bytes().unwrap())
    {
        Ok(_) => {  },
        Err(msg) => { println!("Error writing dev info request!, {}", msg); return ; },
    };

    let device_info_read = read_frame(&mut serial_port, 7).unwrap();
    println!("{:?}", device_info_read);
    let start_3d = new(vec![0x08, 0x00]);
    match serial_port.write(&start_3d.as_bytes().unwrap())
    {
        Ok(_) => { println!("Started reading frames"); },
        Err(msg) => { println!("Error writing dev info request!, {}", msg); return ; },
    };
    thread::sleep(Duration::from_secs(1));

    let frame_3d = read_frame(&mut serial_port, 14401).unwrap();
    println!("{:?}", frame_3d);

    let stop = new(vec![0x02, 0x00, 0x00]);
    match serial_port.write(&stop.as_bytes().unwrap())
    {
        Ok(_) => { println!("Stopped reading frames"); },
        Err(msg) => { println!("Error writing dev info request!, {}", msg); return ; },
    };
}
