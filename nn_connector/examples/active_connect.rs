use nn_connector::active::FuzzConnector;
use nn_connector::error::Error;
use rand::Rng;

fn main() -> Result<(), Error> {
    let mut conn = FuzzConnector::new("some model".into(), 7879).expect("Cannot connect");
    let mut rng = rand::thread_rng();
    let mut counter = 0;
    loop {
        let input = conn.recv_input().unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        let answer: Vec<u32> = (0..5).map(|_| rng.gen_range(0..input.len() as u32)).collect();
        conn.send_heatmap(answer)?;

        let reward = conn.recv_reward()?;
        if reward < 0.0006_f64 {
            counter += 1;
        }
        else {
            println!("{:?} after {counter} zeros", reward);
            counter = 0;
        }
    }
}
