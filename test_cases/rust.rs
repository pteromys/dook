#[derive(Debug)]
pub enum PotorooTreat<'a> {
    Apple,
    SweetPotato(&'a str),
    Bug,
}

pub trait Treat {
    fn eat(self);
}

impl Treat for PotorooTreat<'_> {
    fn eat(self) {
        println!("all gone!");
    }
}
