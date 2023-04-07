use nn_connector::passive::FuzzConnector;

fn main() {
    let mut conn = FuzzConnector::new(7878).expect("Cannot connect");

    println!("Connected to fuzzer with id: {}", conn.id());

    loop {
        match conn.recv_testcase() {
            Ok(res) => {
                println!("{:?}", res);
                // std::thread::sleep(std::time::Duration::from_millis(5_000));
            },
            Err(e) => {
                eprintln!("{:?}", e);
            }
        }
    }
}