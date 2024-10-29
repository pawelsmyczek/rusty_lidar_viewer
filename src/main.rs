use serialport;
use serialport::{DataBits, FlowControl, Parity, SerialPort, StopBits, TTYPort};

use serde::{Deserialize, Serialize};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
    if self.size <= 7 // serializer adds 8 bytes of padding in certain cases ...
    {
        bytes.drain(5..(5 + 8));
    }
    else    // another magic done by serializer for some larger structures
    {
        bytes.drain(5..(5 + 2));
    }
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

fn read_frame(serial_port : &mut TTYPort, payload_size : u16) -> Result<Frame, ()>
{
    let mut frame = vec![0u8; (payload_size + 6) as usize];

    loop
    {
        serial_port.set_timeout(Duration::from_millis(130)).expect("Couldn't set a tiemout");
        match serial_port.read_exact(&mut frame)
        {
            Ok(_) =>
            {
                let frame_obj = new(frame[5..(payload_size+5) as usize].to_vec());

                if frame_obj.header != &frame[0..3]
                {
                    println!("Failed to deserialize frame header"); return Err(())
                }
                let size : u16 = match bincode::deserialize(&frame[3..5])
                {
                    Ok(size) => size,
                    Err(msg) => { println!("Failed to deserialize size of frame {}", msg); return Err(()) }
                };
                if size != payload_size
                {
                    println!("Failed to deserialize size of frame frame, size is not as expected ( {} )", payload_size); return Err(())
                }
                let checksum = frame[frame.len()-1];
                if frame_obj.checksum != checksum
                {
                    println!("Failed to deserialize checksum, expected ( {} )", checksum); return Err(())
                }

                return Ok(frame_obj);
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
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    }).expect("Error setting Ctrl-C handler");

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

    serial_port.flush().unwrap();

    thread::sleep(Duration::from_secs(1));

    let start_3d = new(vec![0x08, 0x00]);
    match serial_port.write(&start_3d.as_bytes().unwrap())
    {
        Ok(_) => { println!("Started reading frames"); },
        Err(msg) => { println!("Error writing dev info request!, {}", msg); return ; },
    };
    thread::sleep(Duration::from_secs(1));

    while running.load(Ordering::SeqCst)
    {
        const point_cloud_3d_size : u16 = 160 * 60;
        let frame_3d = match read_frame(&mut serial_port, ((point_cloud_3d_size*3)/2)+1)
        {
            Ok(frame_3d) => frame_3d,
            Err(msg) => { println!("Failed to read frame : {:?}", msg); break; }
        };
        let mut point_cloud_3d = [0u16;point_cloud_3d_size as usize];
        let mut iter_frame: usize = 0;
        let mut iter_point_cloud: usize = 0;
        while iter_point_cloud < point_cloud_3d_size as usize && iter_frame < frame_3d.payload.len()-3
        {
            let first = frame_3d.payload[iter_frame]; iter_frame+=1;
            let second = frame_3d.payload[iter_frame]; iter_frame+=1;
            let third = frame_3d.payload[iter_frame]; iter_frame+=1;

            point_cloud_3d[iter_point_cloud] = first as u16;
            point_cloud_3d[iter_point_cloud] |= ((second & 0xf) as u16) << 8;
            iter_point_cloud+=1;

            point_cloud_3d[iter_point_cloud] = ((second & 0xf) >> 4) as u16;
            point_cloud_3d[iter_point_cloud] |= (third << 4) as u16;
        }
        thread::sleep(Duration::from_millis(20));
        println!("Read frame, its point cloud is {:?}", point_cloud_3d);
    }

    let stop = new(vec![0x02, 0x00, 0x00]);
    match serial_port.write(&stop.as_bytes().unwrap())
    {
        Ok(_) => { println!("Stopped reading frames"); },
        Err(msg) => { println!("Error writing dev info request!, {}", msg); return ; },
    };
}
