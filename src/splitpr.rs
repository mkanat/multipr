use std::error::Error;
use std::io;

use patch::Patch;

fn main() -> Result<(), Box<dyn Error>> {
    let input = io::read_to_string(io::stdin())?;
    let patch = Patch::from_multiple(&input).map_err(|e| e.to_string())?;
    println!("{:#?}", patch);
    Ok(())
}
