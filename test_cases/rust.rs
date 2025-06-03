#[derive(Debug)]
pub enum PotorooTreat<'a> {
    Apple,
    SweetPotato(&'a str),
    Bug { head: u8, thorax: u8, abdomen: u8 },
}

pub trait Treat {
    fn eat(self);
}

impl Treat for PotorooTreat<'_> {
    fn eat(self) {
        println!("all gone!");
    }
}

fn make_bug() -> PotorooTreat<'static> {
    let abdomen: u8 = 2;
    PotorooTreat::Bug {
        head: 0,
        thorax: 1,
        abdomen,
    }
}

pub use make_bug as hatch;
