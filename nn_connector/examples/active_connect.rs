use nn_connector::active::FuzzConnector;
use nn_connector::error::Error;
use rand::Rng;

fn main() -> Result<(), Error> {
    let mut conn = FuzzConnector::new("some model".into(), 7879).expect("Cannot connect");
    let mut rng = rand::thread_rng();
    loop {
        // get input + original map with first message
        let hashmap = conn.recv_input().unwrap();
        let input = hashmap.get("input").unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        
        let answer: Vec<u32> = (0..5)
            .map(|_| rng.gen_range(0..input.len() as u32))
            .collect();
        conn.send_heatmap(answer)?;

        // get map + input (empty if no debug_mutation feature enabled)
        let hashmap = conn.recv_map()?;
        println!("{hashmap:#?}");
    }
}
