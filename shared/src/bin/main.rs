use eth_consensus_layer_ssz::Temp;

fn main() {
    let temp = Temp { a: 42 };
    println!("Hello, world {:}!", temp.a);
}
