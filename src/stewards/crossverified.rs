//! A wrapper around two different TimeSteward types, to verify that they behave consistently.
//!
//!
//!


use {DeterministicRandomId, FieldId, ExtendedTime, Basics, FieldRc, TimeSteward,
     IncrementalTimeSteward, TimeStewardFromConstants, TimeStewardFromSnapshot, FullTimeSteward, CanonicalTimeSteward, FiatEventOperationError, ValidSince};
use std::collections::HashMap;
use std::cmp::max;
use std::marker::PhantomData;
use Snapshot as SuperSnapshot;
use implementation_support::common::fields_are_equal;

pub struct Steward<B: Basics, Steward0: TimeSteward<Basics = B>, Steward1: TimeSteward<Basics = B> > (
  Steward0,
  Steward1,
  PhantomData <B::Constants>,
);
pub struct Snapshot<B: Basics, Steward0: TimeSteward<Basics = B>, Steward1: TimeSteward<Basics = B> > (
  <Steward0 as TimeSteward>::Snapshot,
  <Steward1 as TimeSteward>::Snapshot,
);


impl<B: Basics, Steward0: TimeSteward<Basics = B>, Steward1: TimeSteward<Basics = B> > ::Accessor for Snapshot<B, Steward0, Steward1> {
  type Basics = B;
// macro_rules! forward_snapshot_method ($method: ident ($self, $($argument_name: ident: $argument_type:ty),*)->$return_type:ty
// TODO: forward all the methods properly
// and check equality by serialization
  fn generic_data_and_extended_last_change (&self, id: FieldId)->Option <(& FieldRc, & ExtendedTime <B>)> {
    match (
      self.0.generic_data_and_extended_last_change (id),
      self.1.generic_data_and_extended_last_change (id)
    ) {
      (None, None) => None,
      (Some (value_0), Some (value_1)) => {
        assert_eq!(value_0.1, value_1.1, "Snapshots returned different last change times for the same field; one or both of the stewards is buggy, or the caller submitted very nondeterministic event/predictor types");
        assert!(fields_are_equal::<B> (id.column_id, value_0.0, value_1.0), "Snapshots returned the same field with the same last change times but different data; one or both of the stewards is buggy, or the caller submitted very nondeterministic event/predictor types");
        Some (value_0)
      },
      _=> panic! ("One snapshot returned a value and the other didn't; one or both of the stewards is buggy, or the caller submitted very nondeterministic event/predictor types")
    }
  }
  fn constants(&self) -> &B::Constants {
// constants methods are usually implemented trivially; we don't bother checking them.
// Since the user only gives you one set of constants, it's hard to return a wrong value
    self.0.constants()
  }
  fn unsafe_now(&self) -> &B::Time {
    let result = (self.0.unsafe_now(), self.1.unsafe_now());
    assert! (result.0 == result.1, "Snapshots returned different times; this is an egregious bug!");
    result.0
  }
}

impl<B: Basics, Steward0: TimeSteward<Basics = B>, Steward1: TimeSteward<Basics = B> > ::MomentaryAccessor for Snapshot<B, Steward0, Steward1> {}

impl<B: Basics, Steward0: TimeSteward<Basics = B>, Steward1: TimeSteward<Basics = B> > ::Snapshot for Snapshot<B, Steward0, Steward1> {
  fn num_fields(&self) -> usize {
    assert_eq!(self.0.num_fields(), self.1.num_fields());
    self.0.num_fields()
  }
}

pub struct SnapshotIter <'a, B: Basics, Steward0: TimeSteward<Basics = B> + 'a, Steward1: TimeSteward<Basics = B> + 'a>
where & 'a Steward0::Snapshot: IntoIterator <Item = ::SnapshotEntry <'a, B>>,
& 'a Steward1::Snapshot: IntoIterator <Item = ::SnapshotEntry <'a, B>>
{
  iter: <& 'a Steward0::Snapshot as IntoIterator>::IntoIter,
  #[allow (dead_code)]
  snapshot: & 'a Snapshot <B, Steward0, Steward1>,
}
impl <'a, B: Basics, Steward0: TimeSteward<Basics = B>, Steward1: TimeSteward<Basics = B>> Iterator for SnapshotIter<'a, B, Steward0, Steward1>
where & 'a Steward0::Snapshot: IntoIterator <Item = ::SnapshotEntry <'a, B>>,
& 'a Steward1::Snapshot: IntoIterator <Item = ::SnapshotEntry <'a, B>> {
  type Item = (FieldId, (& 'a FieldRc, & 'a ExtendedTime <B>));
  fn next (&mut self)->Option <Self::Item> {
    self. iter.next()
  }
  fn size_hint (&self)->(usize, Option <usize>) {self. iter.size_hint()}
}
impl <'a, B: Basics, Steward0: TimeSteward<Basics = B>, Steward1: TimeSteward<Basics = B>> IntoIterator for & 'a Snapshot <B, Steward0, Steward1>
where & 'a Steward0::Snapshot: IntoIterator <Item = ::SnapshotEntry <'a, B>>,
& 'a Steward1::Snapshot: IntoIterator <Item = ::SnapshotEntry <'a, B>>,

 {
  type Item = (FieldId, (& 'a FieldRc, & 'a ExtendedTime <B>));
  type IntoIter = SnapshotIter <'a, B, Steward0, Steward1>;
  fn into_iter (self)->Self::IntoIter {
    assert_eq! (self.0.num_fields(), self.1.num_fields());
    let mut fields = HashMap::new();
    for (id, data) in & self.0 {
      fields.insert (id, (data.0.clone(), data.1.clone()));
    }
    for (id, data) in & self.1 {
      let other_data = fields.get (& id).expect ("field existed in Steward1 snapshot but not Steward0 snapshot");
      assert_eq!(*data .1, other_data .1, "Snapshots returned different last change times for the same field; one or both of the stewards is buggy, or the caller submitted very nondeterministic event/predictor types");
      assert!(fields_are_equal::<B> (id.column_id, data .0, & other_data .0), "Snapshots returned the same field with the same last change times but different data; one or both of the stewards is buggy, or the caller submitted very nondeterministic event/predictor types");
    }
    SnapshotIter:: <'a, B, Steward0, Steward1> {
      iter: (& self.0).into_iter(),
      snapshot: self
    }
  }
}

impl<B: Basics, Steward0: TimeSteward<Basics = B> , Steward1: TimeSteward<Basics = B> > TimeSteward for Steward<B, Steward0, Steward1> {
  type Basics = B;
  type Snapshot = Snapshot<B, Steward0, Steward1>;

  fn valid_since(&self) -> ValidSince<B::Time> {
    max (self.0.valid_since(), self.1.valid_since())
  }
  
  fn insert_fiat_event <E: ::Event <Basics = B>> (&mut self,
                       time: B::Time,
                       id: DeterministicRandomId,
                       event: E)
                       -> Result<(), FiatEventOperationError> {
    time_steward_common_insert_fiat_event_prefix!(B, self, time, E);
    let old_valid_since = (self.0.valid_since(), self.1.valid_since());
    let result = match (
      self.0.insert_fiat_event (time.clone(), id, event.clone()),
      self.1.insert_fiat_event (time, id, event)
    ){
      (Ok (()), Ok (())) => Ok (()),
      (Err (FiatEventOperationError::InvalidTime),_) => panic!("Steward0 returned InvalidTime after its own ValidSince"),
      (_,Err (FiatEventOperationError::InvalidTime)) => panic!("Steward1 returned InvalidTime after its own ValidSince"),
      (Err (FiatEventOperationError::InvalidInput),Err (FiatEventOperationError::InvalidInput)) => Err (FiatEventOperationError::InvalidInput),
      _=> panic!("stewards returned different results for insert_fiat_event; I believe this is ALWAYS a bug in one of the stewards (that is, it cannot be caused by invalid input)"),
    };
    assert!(self .0.valid_since() == old_valid_since.0, "Steward0 broke the ValidSince rules");
    assert!(self .1.valid_since() == old_valid_since.1, "Steward1 broke the ValidSince rules");
    result
  }
  fn remove_fiat_event(&mut self,
                      time: &B::Time,
                      id: DeterministicRandomId)
                      -> Result<(), FiatEventOperationError> {
    if self.valid_since() > *time {
      return Err(FiatEventOperationError::InvalidTime);
    }
    let old_valid_since = (self.0.valid_since(), self.1.valid_since());
    let result = match (
      self.0.remove_fiat_event (time, id),
      self.1.remove_fiat_event (time, id)
    ){
      (Ok (()), Ok (())) => Ok (()),
      (Err (FiatEventOperationError::InvalidTime),_) => panic!("Steward0 returned InvalidTime after its own ValidSince"),
      (_,Err (FiatEventOperationError::InvalidTime)) => panic!("Steward1 returned InvalidTime after its own ValidSince"),
      (Err (FiatEventOperationError::InvalidInput),Err (FiatEventOperationError::InvalidInput)) => Err (FiatEventOperationError::InvalidInput),
      _=> panic!("stewards returned different results for insert_fiat_event; I believe this is ALWAYS a bug in one of the stewards (that is, it cannot be caused by invalid input)"),
    };
    assert!(self .0.valid_since() == old_valid_since.0, "Steward0 broke the ValidSince rules");
    assert!(self .1.valid_since() == old_valid_since.1, "Steward1 broke the ValidSince rules");
    result
  }

  fn snapshot_before<'b>(&'b mut self, time: &'b B::Time) -> Option<Self::Snapshot> {
    if self.valid_since() > *time {
      return None;
    }
    let result = match (
      self.0.snapshot_before (time),
      self.1.snapshot_before (time)
    ) {
      (Some (snapshot_0), Some (snapshot_1)) => Some (Snapshot (snapshot_0, snapshot_1)),
      (None, _) => panic! ("Steward0 failed to return a snapshot at a time it claims to be valid"),
      (_, None) => panic! ("Steward1 failed to return a snapshot at a time it claims to be valid"),
    };
    assert!(self .0.valid_since() <*time, "Steward0 broke the ValidSince rules");
    assert!(self .1.valid_since() <*time, "Steward1 broke the ValidSince rules");
    result
  }
}

impl<B: Basics, Steward0: TimeStewardFromConstants <Basics = B> , Steward1: TimeStewardFromConstants <Basics = B> > TimeStewardFromConstants for Steward<B, Steward0, Steward1> {
  fn from_constants(constants: B::Constants) -> Self {
    let result = Steward::<B, Steward0, Steward1> (
      Steward0::from_constants(constants.clone()),
      Steward1::from_constants(constants),
      PhantomData,
    );
    assert!(result.0.valid_since() == ValidSince::TheBeginning, "Steward0 broke the ValidSince rules");
    assert!(result.1.valid_since() == ValidSince::TheBeginning, "Steward1 broke the ValidSince rules");
    result
  }
}
impl<B: Basics, Steward0: TimeStewardFromSnapshot <Basics = B> , Steward1: TimeStewardFromSnapshot <Basics = B> > TimeStewardFromSnapshot for Steward<B, Steward0, Steward1> {
  fn from_snapshot<'a, S: ::Snapshot<Basics = B>>(snapshot: & 'a S)
                                              -> Self
                                              where & 'a S: IntoIterator <Item = ::SnapshotEntry <'a, B>> {
    let result = Steward (
      Steward0::from_snapshot::<'a, S>(snapshot),
      Steward1::from_snapshot::<'a, S>(snapshot),
      PhantomData,
    );
    assert!(result.0.valid_since() == ValidSince::Before (snapshot.now().clone()), "Steward0 broke the ValidSince rules");
    assert!(result.1.valid_since() == ValidSince::Before (snapshot.now().clone()), "Steward1 broke the ValidSince rules");
    result
  }
}


impl<B: Basics, Steward0: IncrementalTimeSteward <Basics = B>, Steward1: IncrementalTimeSteward <Basics = B>> ::IncrementalTimeSteward for Steward<B, Steward0, Steward1> {
  fn step(&mut self) {
    let updated_0 = self.0.updated_until_before();
    let updated_1 = self.1.updated_until_before();
    if let Some (time_0) = updated_0 {
      if updated_1.as_ref().map_or (true, | time_1 | time_0 < *time_1) {
// println!("stepping 0");
        let old_valid_since = self.0.valid_since();
        let strict = old_valid_since > time_0;
        self.0.step();
        let new_valid_since = self.0.valid_since();
        assert!(new_valid_since <= old_valid_since || new_valid_since <= ValidSince::After (time_0), "Steward0 broke the ValidSince rules");
        if strict { assert!(new_valid_since <= old_valid_since, "Steward0 broke the ValidSince rules"); }
        return;
      }
    }
    if let Some (time_1) = updated_1 {
// println!("stepping 1");
      let old_valid_since = self.1.valid_since();
      let strict = old_valid_since > time_1;
      self.1.step();
      let new_valid_since = self.1.valid_since();
      assert!(new_valid_since <= old_valid_since || new_valid_since <= ValidSince::After (time_1), "Steward1 broke the ValidSince rules");
      if strict { assert!(new_valid_since <= old_valid_since, "Steward1 broke the ValidSince rules"); }
    }
  }
  fn updated_until_before (&self)->Option <B::Time> {
    match (self.0.updated_until_before(), self.1.updated_until_before()) {
      (None, None) => None,
      (Some (time_0), None) => {
        Some (time_0)
      },
      (None, Some (time_1)) => {
        Some (time_1)
      },
      (Some (time_0), Some (time_1)) => {
        if time_0 <= time_1 {
          Some (time_0)
        } else {
          Some (time_1)
        }
      },
    }
  }
}

impl<B: Basics, Steward0: FullTimeSteward <Basics = B>, Steward1: FullTimeSteward <Basics = B>> FullTimeSteward for Steward<B, Steward0, Steward1> {}
impl<B: Basics, Steward0: CanonicalTimeSteward <Basics = B>, Steward1: CanonicalTimeSteward <Basics = B>> CanonicalTimeSteward for Steward<B, Steward0, Steward1> {}
