#[macro_use]
extern crate time_steward;

extern crate rand;
extern crate bincode;

extern crate serde;
#[macro_use]
extern crate serde_derive;

use time_steward::{TimeSteward, TimeStewardFromConstants, TimeStewardFromSnapshot, DeterministicRandomId, Column, ColumnId, RowId, PredictorId, EventId,
     ColumnType, EventType, PredictorType};
use time_steward::stewards::{inefficient_flat, memoized_flat, amortized, flat_to_inefficient_full, crossverified};


type Time = i64;

const HOW_MANY_PHILOSOPHERS: i32 = 7;

time_steward_basics!(struct Basics {
  type Time = Time;
  type Constants = ();
  type IncludedTypes = TimeStewardTypes;
});

#[derive (Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
struct Philosopher {
  // This is sometimes in the future because
  // they muse philosophically about handshakes
  // for a while, whenever one of them happens.
  time_when_next_initiates_handshake: Time,
}
impl Column for Philosopher {
  type FieldType = Self;
  fn column_id() -> ColumnId {
    ColumnId(0x4084d1501468b6dd)
  }
}
 

fn get_philosopher_id(index: i32) -> RowId {
  DeterministicRandomId::new(&(0x2302c38efb47e0d0u64, index))
}

type TimeStewardTypes = (ColumnType<Philosopher>,
                         EventType<Initialize>,
                         EventType<Tweak>,
                         EventType<TweakUnsafe>,
                         EventType<Shake>,
                         PredictorType<Shaker>);

fn display_snapshot<S: time_steward::Snapshot<Basics = Basics>>(snapshot: &S) {
  println!("snapshot for {}", snapshot.now());
  for index in 0..HOW_MANY_PHILOSOPHERS {
    println!("{}",
             snapshot.get::<Philosopher>(get_philosopher_id(index))
               .expect("missing philosopher")
               .time_when_next_initiates_handshake);
  }
}

time_steward_predictor! (
  struct Shaker, Basics, PredictorId(0x0e7f27c7643f8167), watching Philosopher,
  | pa, whodunnit | {
// println!("Planning {}", whodunnit);
  let me = pa.get::<Philosopher>(whodunnit).unwrap().clone();
  pa.predict_at_time(me.time_when_next_initiates_handshake, Shake::new (whodunnit));
});

time_steward_event! (
  struct Shake {whodunnit: RowId}, Basics, EventId (0x8987a0b8e7d3d624),
  | &self, m | {
    let now = *m.now();
    let friend_id = get_philosopher_id(m.gen_range(0, HOW_MANY_PHILOSOPHERS));
    let awaken_time_1 = now + m.gen_range(-1, 4);
    let awaken_time_2 = now + m.gen_range(-1, 7);
// println!("SHAKE!!! @{}. {}={}; {}={}", now, self.whodunnit, awaken_time_2, friend_id, awaken_time_1);
// IF YOU SHAKE YOUR OWN HAND YOU RECOVER
// IN THE SECOND TIME APPARENTLY
    m.set::<Philosopher>(friend_id,
                             Some(Philosopher {
                               time_when_next_initiates_handshake: awaken_time_1,
                             }));
    m.set::<Philosopher>(self.whodunnit,
                             Some(Philosopher {
                               time_when_next_initiates_handshake: awaken_time_2,
                             }));
  }
);

time_steward_event! (
  struct Initialize {}, Basics, EventId (0xd5e73d8ba6ec59a2),
  | &self, m | {
    println!("FIAT!!!!!");
    for i in 0..HOW_MANY_PHILOSOPHERS {
      m.set::<Philosopher>(get_philosopher_id(i),
        Some(Philosopher {
          time_when_next_initiates_handshake: (i + 1) as Time,
        })
      );
    }
  }
);

time_steward_event! (
  struct Tweak {}, Basics, EventId (0xfe9ff3047f9a9552),
  | &self, m | {
    println!(" Tweak !!!!!");
    let now = *m.now();
    let friend_id = get_philosopher_id(m.gen_range(0, HOW_MANY_PHILOSOPHERS));
    let awaken_time = now + m.gen_range(-1, 7);

    m.set::<Philosopher>(friend_id,
                             Some(Philosopher {
                               time_when_next_initiates_handshake: awaken_time,
                             }));
  }
);

use rand::{Rng, SeedableRng, ChaChaRng};
thread_local! {static INCONSISTENT: u32 = rand::thread_rng().gen::<u32>();}

time_steward_event! (
  struct TweakUnsafe {}, Basics, EventId (0xa1618440808703da),
  | &self, m | {
    println!(" TweakUnsafe !!!!!");
    let now = *m.now();
    let friend_id = get_philosopher_id(m.gen_range(0, HOW_MANY_PHILOSOPHERS));

    let inconsistent = INCONSISTENT.with (| value | {
      *value
    });
    let mut rng = ChaChaRng::from_seed (& [inconsistent, m. next_u32()]);
    let awaken_time = now + rng.gen_range(-1, 7);

    m.set::<Philosopher>(friend_id,
                             Some(Philosopher {
                               time_when_next_initiates_handshake: awaken_time,
                             }));
  }
);

#[test]
pub fn handshakes_simple() {
  type Steward = crossverified::Steward<Basics, inefficient_flat::Steward<Basics>, memoized_flat::Steward<Basics>>;
  let mut stew: Steward = Steward::from_constants(());

  stew.insert_fiat_event(0,
                       DeterministicRandomId::new(&0x32e1570766e768a7u64),
                       Initialize::new())
    .unwrap();
    
  for increment in 1..21 {
    let snapshot: <Steward as TimeSteward>::Snapshot = stew.snapshot_before(&(increment * 100i64)).unwrap();
    display_snapshot(&snapshot);
  }
}

#[test]
pub fn handshakes_reloading() {
  type Steward = crossverified::Steward<Basics, amortized::Steward<Basics>, memoized_flat::Steward<Basics>>;
  let mut stew: Steward = Steward::from_constants(());

  stew.insert_fiat_event(0,
                       DeterministicRandomId::new(&0x32e1570766e768a7u64),
                       Initialize::new())
    .unwrap();

  let mut snapshots = Vec::new();
  for increment in 1..21 {
    snapshots.push(stew.snapshot_before(&(increment * 100i64)));
    stew = Steward::from_snapshot::<<Steward as TimeSteward>::Snapshot> (snapshots.last().unwrap().as_ref().unwrap());
  }
  for snapshot in snapshots.iter_mut()
    .map(|option| option.as_mut().expect("all these snapshots should have been valid")) {
    display_snapshot(snapshot);
    let mut writer: Vec<u8> = Vec::with_capacity(128);
    time_steward::serialize_snapshot:: <Basics, <Steward as TimeSteward>::Snapshot,_,_> (snapshot, &mut writer, bincode::Infinite).unwrap();
    // let serialized = String::from_utf8 (serializer.into_inner()).unwrap();
    println!("{:?}", writer);
    use std::io::Cursor;
    let mut reader = Cursor::new(writer);
    let deserialized = time_steward::deserialize_snapshot:: <Basics, _,_> (&mut reader, bincode::Infinite/*serialized.as_bytes().iter().map (| bite | Ok (bite.clone()))*/).unwrap();
    println!("{:?}", deserialized);
    display_snapshot(&deserialized);
    use time_steward::MomentaryAccessor;
    display_snapshot(&Steward::from_snapshot::<time_steward::FiatSnapshot<Basics>>(&deserialized).snapshot_before(deserialized.now()).unwrap());
  }
  // panic!("anyway")
}

#[test]
fn handshakes_retroactive() {
  type Steward = crossverified::Steward<Basics, amortized::Steward<Basics>, flat_to_inefficient_full::Steward<Basics, memoized_flat::Steward <Basics> >>;
  let mut stew: Steward = Steward::from_constants(());

  stew.insert_fiat_event(0,
                       DeterministicRandomId::new(&0x32e1570766e768a7u64),
                       Initialize::new())
    .unwrap();

  stew.snapshot_before(&(2000i64));
  for increment in 1..21 {
    stew.insert_fiat_event(increment * 100i64, DeterministicRandomId::new(&increment), Tweak::new()).unwrap();
    let snapshot: <Steward as TimeSteward>::Snapshot = stew.snapshot_before(&(2000i64)).unwrap();
    display_snapshot(&snapshot);
  }

}

#[test]
fn local_synchronization_test() {
  use time_steward::stewards::simply_synchronized;
  use std::net::{TcpListener, TcpStream};
  use std::io::{BufReader, BufWriter};
  let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
  let port = listener.local_addr().unwrap().port();
  ::std::thread::spawn(move || {
    let end_0 = listener.accept().unwrap().0;
    let mut stew_0: simply_synchronized::Steward<Basics, amortized::Steward<Basics>> =
      simply_synchronized::Steward::new(DeterministicRandomId::new(&0u32),
                                        0,
                                        4,
                                        (),
                                        BufReader::new(end_0.try_clone().unwrap()),
                                        BufWriter::new(end_0));
    stew_0.insert_fiat_event(0,
                         DeterministicRandomId::new(&0x32e1570766e768a7u64),
                         Initialize::new())
      .unwrap();

    for increment in 1..21 {
      let time = increment * 100i64;
      if increment % 3 == 0 {
        stew_0.insert_fiat_event(time, DeterministicRandomId::new(&increment), Tweak::new())
          .unwrap();
      }
      stew_0.snapshot_before(&time);
      stew_0.settle_before(time);
    }
    stew_0.finish();
  });
  let end_1 = TcpStream::connect(("127.0.0.1", port)).unwrap();
  let mut stew_1: simply_synchronized::Steward<Basics, amortized::Steward<Basics>> =
    simply_synchronized::Steward::new(DeterministicRandomId::new(&1u32),
                                      0,
                                      4,
                                      (),
                                      BufReader::new(end_1.try_clone().unwrap()),
                                      BufWriter::new(end_1));

  for increment in 1..21 {
    let time = increment * 100i64;
    if increment % 4 == 0 {
      stew_1.insert_fiat_event(time, DeterministicRandomId::new(&increment), Tweak::new()).unwrap();
    }
    stew_1.snapshot_before(&time);
    stew_1.settle_before(time);
  }
  stew_1.finish();
}

#[test]
#[should_panic (expected = "event occurred this way locally")]
fn local_synchronization_failure() {
  use time_steward::stewards::simply_synchronized;
  use std::net::{TcpListener, TcpStream};
  use std::io::{BufReader, BufWriter};
  let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
  let port = listener.local_addr().unwrap().port();
  ::std::thread::spawn(move || {
    let end_0 = listener.accept().unwrap().0;
    let mut stew_0: simply_synchronized::Steward<Basics, amortized::Steward<Basics>> =
      simply_synchronized::Steward::new(DeterministicRandomId::new(&0u32),
                                        0,
                                        4,
                                        (),
                                        BufReader::new(end_0.try_clone().unwrap()),
                                        BufWriter::new(end_0));
    stew_0.insert_fiat_event(0,
                         DeterministicRandomId::new(&0x32e1570766e768a7u64),
                         Initialize::new())
      .unwrap();

    for increment in 1..21 {
      let time = increment * 100i64;
      if increment % 3 == 0 {
        stew_0.insert_fiat_event(time,
                             DeterministicRandomId::new(&increment),
                             TweakUnsafe::new())
          .unwrap();
      }
      stew_0.snapshot_before(&time);
      stew_0.settle_before(time);
    }
    stew_0.finish();
  });
  let end_1 = TcpStream::connect(("127.0.0.1", port)).unwrap();
  let mut stew_1: simply_synchronized::Steward<Basics, amortized::Steward<Basics>> =
    simply_synchronized::Steward::new(DeterministicRandomId::new(&1u32),
                                      0,
                                      4,
                                      (),
                                      BufReader::new(end_1.try_clone().unwrap()),
                                      BufWriter::new(end_1));

  for increment in 1..21 {
    let time = increment * 100i64;
    if increment % 4 == 0 {
      stew_1.insert_fiat_event(time,
                           DeterministicRandomId::new(&increment),
                           TweakUnsafe::new())
        .unwrap();
    }
    stew_1.snapshot_before(&time);
    stew_1.settle_before(time);
  }
  stew_1.finish();
}
