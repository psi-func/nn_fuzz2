use nn_connector::connector::FuzzConnector;

fn main() {
    let mut conn = FuzzConnector::new(7878).expect("Cannot connect");

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