//! A full TimeSteward implementation that has decent (amortized) asymptotic performance for all common operations.
//!
//! This is intended to be the simplest possible implementation that
//! meets those conditions. As such, it's not especially optimized.
//! Here are some of its specific weaknesses:
//!
//! * no support for multithreading
//! * when a field changes in the past, this TimeSteward immediately erases all
//! more-recent versions of that field. This can take time proportional
//! to the amount of times that field has changed since the past change.
//! (It doesn't affect the amortized time because the recording of
//! each thing amortizes its eventual deletion, but it can cause a hiccup.)
//! * This erasing happens even if the field was overwritten at some point
//! without being examined. In that case, we could theoretically optimize
//! by leaving the future of the field untouched.
//! * There can also be hiccups at arbitrary times when the hash table resizes.
//! * We haven't optimized for the "most changes happen in the present" case,
//! which means we pay a bunch of log n factors when we could be paying O(1).
//! * If you keep around old snapshots of times when no fields are
//! actually being modified anymore, they will eventually have
//! all their data copied into them unnecessarily. This could be avoided
//! if we had a good two-dimensional tree type so that the snapshots
//! could be queried by (SnapshotIdx X BaseTime) rectangles.
//! * There might be more small dependency optimizations we could do,
//! like distinguishing between accessing just a field's data
//! and accessing just its last change time, or even the difference
//! between those and just checking whether the field exists or not,
//! or other conditions (although we would need an API change to track
//! some of those things). However, I suspect that the additional runtime
//! cost of recording these different dependencies wouldn't be worth it.
//! (This would only have a small effect at best, because it wouldn't
//! slow down dependency chain propagation.)
//!
//!

use {DeterministicRandomId, SiphashIdGenerator, RowId, FieldId, PredictorId, TimeId, Column, StewardRc,
     FieldRc, ExtendedTime, Basics, Accessor, FiatEventOperationError, ValidSince, TimeSteward,
     IncrementalTimeSteward, TimeStewardFromConstants};
use implementation_support::common::{self, DynamicEventFn};
use std::collections::{HashMap, BTreeMap, HashSet, BTreeSet};
// use std::collections::Bound::{Included, Excluded, Unbounded};
use std::rc::Rc;
use std::cell::RefCell;
use std::ops::Drop;
use rand::Rng;
use implementation_support::insert_only;
use implementation_support::data_structures::{partially_persistent_nonindexed_set, BuildTrivialU64Hasher};

pub type SnapshotIdx = u64;

#[derive (Debug)]
pub enum EventValidity {
  Invalid,
  ValidWithDependencies(HashSet<FieldId, BuildTrivialU64Hasher>),
}
#[derive (Debug)]
pub struct EventExecutionState {
  pub fields_changed: HashSet<FieldId, BuildTrivialU64Hasher>,
  pub checksum: u64,
  pub validity: EventValidity,
}

pub struct EventState<B: Basics> {
  pub time: ExtendedTime <B>,
  pub schedule: Option<DynamicEvent<B>>,
  pub scheduled_by: Option <(RowId, PredictorId)>,
  pub execution_state: Option<EventExecutionState>,
}

pub fn limit_option_by_value_with_none_representing_positive_infinity <B: Ord + Clone> (first: &mut Option <B>, second: &B) {
  if let Some(value) = first.as_mut() {
    if second < value {
      value.clone_from(second);
    }
  }
  if first.is_none() {
    *first = Some(second.clone());
  }
}


#[derive (Clone)]
pub struct Field<B: Basics> {
  pub last_change: ExtendedTime<B>,
  pub data: Option<FieldRc>,
}
pub type SnapshotField<B> = (FieldRc, ExtendedTime<B>);

// #[derive (Clone)]
// enum AccessInfo {
//  EventAccess,
//  PredictionAccess(RowId, PredictorId),
// }
#[derive (Debug)]
pub struct FieldHistory<B: Basics> {
  pub changes: Vec<Field<B>>,
  pub first_snapshot_not_updated: SnapshotIdx,
}

pub type SnapshotsData<B> = BTreeMap<SnapshotIdx,
                                             (<B as Basics>::Time,
                                              Rc<insert_only::HashMap<FieldId, SnapshotField<B>, BuildTrivialU64Hasher>>)>;

pub struct Fields<B: Basics> {
  pub field_states: HashMap<FieldId, FieldHistory<B>, BuildTrivialU64Hasher>,
  pub changed_since_snapshots: SnapshotsData<B>,
}

#[derive (Default)]
pub struct Dependencies<B: Basics> {
  pub events: BTreeSet<ExtendedTime<B>>,
  pub bounded_predictions: BTreeMap<ExtendedTime<B>, HashSet<(RowId, PredictorId)>>,
  pub unbounded_predictions: HashSet<(RowId, PredictorId), BuildTrivialU64Hasher>,
}
impl<B: Basics> Dependencies<B> {
  pub fn is_empty(&self) -> bool {
    self.events.is_empty() && self.bounded_predictions.is_empty() &&
    self.unbounded_predictions.is_empty()
  }
}
pub type DependenciesMap<B> = HashMap<FieldId, Dependencies<B>, BuildTrivialU64Hasher>;

#[derive (Clone)]
pub struct Prediction<B: Basics> {
  pub predictor_accessed: Vec<FieldId>,
  pub what_will_happen: Option<(ExtendedTime<B>, DynamicEvent<B>)>,
  pub made_at: ExtendedTime<B>,
  pub valid_until: Option<ExtendedTime<B>>,
}
#[derive (Default, Debug)]
pub struct PredictionHistory<B: Basics> {
  pub next_needed: Option<ExtendedTime<B>>,
  pub predictions: Vec<Prediction<B>>,
}

use std::fmt;
impl <B: Basics> fmt::Debug for Prediction <B> {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    try! (write!(f, "Prediction {{ made_at: {:?}, valid_until: {:?}", self.made_at, self.valid_until));
    if let Some (&(ref event_time, ref event)) = self.what_will_happen.as_ref() {
      try! (write!(f, ", what_will_happen: ({:?}, DynamicEvent with {:?}", event_time, event.event_id()));
    }
    write!(f, " }}")
  }
}
impl <B: Basics> fmt::Debug for Field <B> {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "Field {{ last_change: {:?} }}", self.last_change)
  }
}

pub struct StewardShared<B: Basics> {
  pub settings: Settings<B>,
  pub constants: B::Constants,
  pub fields: RefCell<Fields<B>>,
}

pub struct StewardEventsInfo<B: Basics> {
  pub event_states: HashMap<TimeId, EventState<B>, BuildTrivialU64Hasher>,
  pub events_needing_attention: BTreeSet<ExtendedTime<B>>,
  pub dependencies: DependenciesMap<B>,
}

pub struct StewardOwned<B: Basics> {
  pub events: StewardEventsInfo<B>,

  pub invalid_before: ValidSince<B::Time>,
  pub next_snapshot: SnapshotIdx,
  pub existent_fields: partially_persistent_nonindexed_set::Set<FieldId, BuildTrivialU64Hasher>,

  pub predictions_by_id: HashMap<(RowId, PredictorId), PredictionHistory<B>>,
  pub predictions_missing_by_time: BTreeMap<ExtendedTime<B>, HashSet<(RowId, PredictorId), BuildTrivialU64Hasher>>,

  pub checksum_info: Option<ChecksumInfo<B>>,
}

pub struct Steward<B: Basics> {
  pub(super) owned: StewardOwned<B>,
  pub(super) shared: Rc<StewardShared<B>>,
}
pub struct Snapshot<B: Basics> {
  pub(super) now: B::Time,
  pub(super) index: SnapshotIdx,
  pub(super) field_states: Rc<insert_only::HashMap<FieldId, SnapshotField<B>, BuildTrivialU64Hasher>>,
  pub(super) shared: Rc<StewardShared<B>>,
  pub(super) num_fields: usize,
  pub(super) field_ids: partially_persistent_nonindexed_set::Snapshot<FieldId>,
}
pub struct MutatorResults<B: Basics> {
  pub fields: insert_only::HashMap<FieldId, Field<B>, BuildTrivialU64Hasher>,
  pub checksum_generator: RefCell<SiphashIdGenerator>,
}
pub struct Mutator<'a, B: Basics> {
  pub shared: &'a StewardShared<B>,
  pub fields: &'a Fields<B>,
  pub generic: common::GenericMutator<B>,
  pub results: MutatorResults<B>,
}
pub struct PredictorAccessorResults<B: Basics> {
  pub valid_until: Option<ExtendedTime<B>>,
  pub used_unsafe_now: bool,
}
pub struct PredictorAccessor<'a, B: Basics> {
  // predictor_id: PredictorId,
  // about_row_id: RowId,
  pub internal_now: ExtendedTime<B>,
  pub shared: &'a StewardShared<B>,
  pub fields: &'a Fields<B>,
  pub generic: common::GenericPredictorAccessor<B, DynamicEvent<B>>,
  pub results: RefCell<PredictorAccessorResults<B>>,
}

time_steward_common_dynamic_callback_structs! (pub, Mutator, PredictorAccessor, DynamicEvent, DynamicPredictor, Settings);

impl<B: Basics> Drop for Snapshot<B> {
  fn drop(&mut self) {
    self.shared.fields.borrow_mut().changed_since_snapshots.remove(&self.index);
  }
}

impl<B: Basics> ::Accessor for Snapshot<B> {
  type Basics = B;
  fn generic_data_and_extended_last_change(&self,
                                           id: FieldId)
                                           -> Option<(&FieldRc, &ExtendedTime<B>)> {
    self.field_states
      .get_default(id, || {
        self.shared
          .fields
          .borrow()
          .get_for_snapshot(id, &self.now, self.index)
      })
      .map(|whatever| (&whatever.0, &whatever.1))
  }
  fn constants(&self) -> &B::Constants {
    &self.shared.constants
  }
  fn unsafe_now(&self) -> &B::Time {
    &self.now
  }
}
impl<'a, B: Basics> ::Accessor for Mutator<'a, B> {
  type Basics = B;
  fn generic_data_and_extended_last_change(&self,
                                           id: FieldId)
                                           -> Option<(&FieldRc, &ExtendedTime<B>)> {
    let field = self.results
      .fields
      .get_default(id, || {
        Some(match self.fields.get(id, &self.generic.now, false) {
          None => {
            Field {
              data: None,
              last_change: self.generic.now.clone(),
            }
          }
          Some((data, time)) => {
            Field {
              data: Some(data.clone()),
              last_change: time.clone(),
            }
          }
        })
      })
      .unwrap();
    field.data.as_ref().map(|data| (data, &field.last_change))
  }
  fn constants(&self) -> &B::Constants {
    &self.shared.constants
  }
  time_steward_common_accessor_methods_for_mutator!(B);
}
impl<'a, B: Basics> PredictorAccessor<'a, B> {
  fn get_impl(&self, id: FieldId) -> Option<(&FieldRc, &ExtendedTime<B>)> {
    self.fields.get_and_next(id, &self.internal_now, true).map(|(result, next)| {
      assert! (*result.1 <= self.internal_now);
      if let Some(limit) = next {
        assert!(*limit >self.internal_now);
        limit_option_by_value_with_none_representing_positive_infinity(&mut self.results
                                                                         .borrow_mut()
                                                                         .valid_until,
                                                                       limit);
      }
      result
    })
  }
}
impl<'a, B: Basics> ::Accessor for PredictorAccessor<'a, B> {
  type Basics = B;
  time_steward_common_accessor_methods_for_predictor_accessor!(B, get_impl);
  fn constants(&self) -> &B::Constants {
    &self.shared.constants
  }
  fn unsafe_now(&self) -> &B::Time {
    self.results.borrow_mut().used_unsafe_now = true;
    &self.internal_now.base
  }
}

impl<B: Basics> ::MomentaryAccessor for Snapshot<B> {}
impl<'a, B: Basics> ::MomentaryAccessor for Mutator<'a, B> {}
impl<'a, B: Basics> ::PredictorAccessor for PredictorAccessor<'a, B> {
  time_steward_common_predictor_accessor_methods_for_predictor_accessor!(B, DynamicEventFn);
}

impl<B: Basics> ::Snapshot for Snapshot<B> {
  fn num_fields(&self) -> usize {
    // TODO: optimize
    self.into_iter().count()
  }
}

pub struct SnapshotIter<'a, B: Basics>(partially_persistent_nonindexed_set::SnapshotIter<'a,
                                                                                         FieldId>,
                                       &'a Snapshot<B>);
impl<'a, B: Basics> Iterator for SnapshotIter<'a, B> {
  type Item = (FieldId, (&'a FieldRc, &'a ExtendedTime<B>));
  fn next(&mut self) -> Option<Self::Item> {
    loop {
      match (self.0).next().map(|id| (id, (self.1).generic_data_and_extended_last_change(id))) {
        None => return None,
        Some((id, Some(value))) => return Some((id, value)),
        _ => (),
      }
    }
  }
  fn size_hint(&self) -> (usize, Option<usize>) {
    (0, Some(self.1.num_fields))
  }
}
impl<'a, B: Basics> IntoIterator for &'a Snapshot<B> {
  type Item = (FieldId, (&'a FieldRc, &'a ExtendedTime<B>));
  type IntoIter = SnapshotIter<'a, B>;
  fn into_iter(self) -> Self::IntoIter {
    SnapshotIter(self.field_ids.iter(), self)
  }
}

impl<'a, B: Basics> ::Mutator for Mutator<'a, B> {
  fn set<C: Column>(&mut self, id: RowId, data: Option<C::FieldType>) {
    time_steward_common_mutator_set_prefix!(B, C, self, id, data);
    let field_id = FieldId::new(id, C::column_id());
    ::bincode::serialize_into(&mut *self.results.checksum_generator.borrow_mut(),
                                     &id,
                                     ::bincode::Infinite).unwrap();
    ::bincode::serialize_into(&mut *self.results.checksum_generator.borrow_mut(),
                                     &data,
                                     ::bincode::Infinite).unwrap();
    self.results.fields.insert(field_id,
                               Field {
                                 last_change: self.generic.now.clone(),
                                 data: data.map(|whatever| {
                                   let something: FieldRc = StewardRc::new(whatever);
                                   something
                                 }),
                               });
  }
  time_steward_common_mutator_methods_for_mutator!(B);
}
impl<'a, B: Basics> Rng for Mutator<'a, B> {
  time_steward_common_rng_methods_for_mutator!(B);
}

impl<B: Basics> Fields<B> {
  pub fn get(&self,
             id: FieldId,
             time: &ExtendedTime<B>,
             after: bool)
             -> Option<(&FieldRc, &ExtendedTime<B>)> {
    self.get_and_next(id, time, after).map(|whatever| whatever.0)
  }
  pub fn get_and_next(&self,
                      id: FieldId,
                      time: &ExtendedTime<B>,
                      after: bool)
                      -> Option<((&FieldRc, &ExtendedTime<B>), Option<&ExtendedTime<B>>)> {
    self.field_states.get(&id).and_then(|history| {
      let index = match history.changes.binary_search_by_key(&time, |change| &change.last_change) {
        Ok(index) => if after { index } else { index.wrapping_sub(1) },
        Err(index) => index.wrapping_sub(1),
      };
      history.changes.get(index).and_then(|change| {
        change.data.as_ref().map(|data| {
          ((data, &change.last_change),
           history.changes.get(index + 1).map(|result| &result.last_change))
        })
      })
    })
  }
  pub fn get_for_snapshot(&self,
                          id: FieldId,
                          time: &B::Time,
                          index: SnapshotIdx)
                          -> Option<(FieldRc, ExtendedTime<B>)> {
    self.field_states.get(&id).and_then(|history| {
      if history.first_snapshot_not_updated > index {
        return None;
      }
      history.previous_change_for_snapshot(time)
    })
  }
}
impl<B: Basics> Steward<B> {
  fn update_until_beginning_of(&mut self, target_time: &B::Time) {
    while self.updated_until_before().map_or(false, |time| time < *target_time) {
      self.do_next();
    }
  }
}


impl<B: Basics> TimeSteward for Steward<B> {
  type Basics = B;
  type Snapshot = Snapshot<B>;

  fn valid_since(&self) -> ValidSince<B::Time> {
    self.owned.invalid_before.clone()
  }

  fn insert_fiat_event<E: ::Event<Basics = B>>(&mut self,
                                               time: B::Time,
                                               id: DeterministicRandomId,
                                               event: E)
                                               -> Result<(), FiatEventOperationError> {
    time_steward_common_insert_fiat_event_prefix!(B, self, time, E);
    match self.owned.events.schedule_event(common::extended_time_of_fiat_event(time, id),
                                           StewardRc::new(DynamicEventFn::new(event)), None) {
      Err(_) => Err(FiatEventOperationError::InvalidInput),
      Ok(_) => Ok(()),
    }
  }

  fn remove_fiat_event(&mut self,
                       time: &B::Time,
                       id: DeterministicRandomId)
                       -> Result<(), FiatEventOperationError> {
    if self.valid_since() > *time {
      return Err(FiatEventOperationError::InvalidTime);
    }
    match self.owned
      .events
      .unschedule_event(&common::extended_time_of_fiat_event(time.clone(), id)) {
      Err(_) => Err(FiatEventOperationError::InvalidInput),
      Ok(_) => Ok(()),
    }
  }

  fn snapshot_before<'b>(&'b mut self, time: &'b B::Time) -> Option<Self::Snapshot> {
    if self.valid_since() > *time {
      return None;
    }
    self.update_until_beginning_of(time);

    let field_states = self.shared
      .fields
      .borrow_mut()
      .changed_since_snapshots
      .entry(self.owned.next_snapshot)
      .or_insert((time.clone(), Rc::new(insert_only::HashMap::default())))
      .1
      .clone();
    let result = Some(Snapshot {
      now: time.clone(),
      index: self.owned.next_snapshot,
      field_states: field_states,
      shared: self.shared.clone(),
      num_fields: self.shared.fields.borrow().field_states.len(),
      field_ids: self.owned.existent_fields.snapshot(),
    });

    self.owned.next_snapshot += 1;
    result
  }
}

impl<B: Basics> IncrementalTimeSteward for Steward<B> {
  fn step(&mut self) {
    self.do_next();
  }
  fn updated_until_before(&self) -> Option<B::Time> {
    match self.next_stuff() {
      (None, None) => None,
      (Some(event_time), None) => Some(event_time.base),
      (None, Some((prediction_time, _, _))) => Some(prediction_time.base),
      (Some(event_time), Some((prediction_time, _, _))) => {
        if event_time <= prediction_time {
          Some(event_time.base)
        } else {
          Some(prediction_time.base)
        }
      }
    }
  }
}
impl<B: Basics> TimeStewardFromConstants for Steward<B> {
  fn from_constants(constants: B::Constants) -> Self {
    Steward {
      owned: StewardOwned {
        events: StewardEventsInfo {
          events_needing_attention: BTreeSet::new(),
          event_states: HashMap::default(),
          dependencies: HashMap::default(),
        },
        invalid_before: ValidSince::TheBeginning,
        next_snapshot: 0,
        existent_fields: partially_persistent_nonindexed_set::Set::default(),
        predictions_missing_by_time: BTreeMap::new(),
        predictions_by_id: HashMap::new(),
        checksum_info: None,
      },
      shared: Rc::new(StewardShared {
        settings: Settings::<B>::new(),
        constants: constants,
        fields: RefCell::new(Fields {
          field_states: HashMap::default(),
          changed_since_snapshots: BTreeMap::new(),
        }),
      }),
    }
  }
}
impl<B: Basics> ::TimeStewardFromSnapshot for Steward<B> {
  fn from_snapshot<'a, S: ::Snapshot<Basics = B>>(snapshot: &'a S) -> Self
    where &'a S: IntoIterator<Item = ::SnapshotEntry<'a, B>>
  {
    let mut result = Self::from_constants(snapshot.constants().clone());
    result.owned.invalid_before = ValidSince::Before(snapshot.now().clone());
    let mut predictions_needed = HashSet::new();
    let mut last_event = None;
    result.shared.fields.borrow_mut().field_states = snapshot.into_iter()
      .map(|(id, stuff)| {
        if match last_event {
          None => true,
          Some(ref time) => stuff.1 > time,
        } {
          last_event = Some(stuff.1.clone());
        }
        result.owned.existent_fields.insert(id);
        result.shared.settings.predictors_by_column.get(&id.column_id).map(|predictors| {
          for predictor in predictors {
            predictions_needed.insert((id.row_id, predictor.predictor_id));
          }
        });
        (id,
         FieldHistory {
          changes: vec![Field {
                  data: Some (stuff.0.clone()),
                  last_change: stuff.1.clone(),
                }],
          first_snapshot_not_updated: 0,
        })
      })
      .collect();
    for &(row_id, predictor_id) in predictions_needed.iter() {
      result.owned.predictions_by_id.insert((row_id, predictor_id),
                                            PredictionHistory {
                                              next_needed: last_event.clone(),
                                              predictions: Vec::new(),
                                            });
      result.owned
        .predictions_missing_by_time
        .entry(last_event.clone().unwrap())
        .or_insert(Default::default())
        .insert((row_id, predictor_id));
    }
    for (row_id, predictor_id) in predictions_needed {
      result.make_prediction(row_id, predictor_id, last_event.as_ref().unwrap());
    }

    result
  }
}
impl<B: Basics> ::FullTimeSteward for Steward<B> {}
impl<B: Basics> ::CanonicalTimeSteward for Steward<B> {}

use std::ops::{Add, Sub, Mul, Div};
pub struct ChecksumInfo<B: Basics> {
  start: B::Time,
  stride: B::Time,
  checksums: Vec<u64>,
}
impl <B: Basics> ::SimpleSynchronizableTimeSteward for Steward <B>
where B::Time: Add <Output = B::Time> + Sub<Output = B::Time> + Mul<i64, Output = B::Time> + Div<B::Time, Output = i64>
{
  fn begin_checks (&mut self, start: B::Time, stride: B::Time) {
    self.owned.checksum_info = Some (ChecksumInfo {
      start: start, stride: stride, checksums: Vec::new()
    });
  }
  fn checksum(&mut self, chunk: i64)->u64 {
    loop {
      if let Some (time) = self.updated_until_before() {
        if (time - self.owned.checksum_info.as_ref().unwrap().start.clone())/self.owned.checksum_info.as_ref().unwrap().stride.clone() > chunk {
          break;
        }
      } else {break;}
      self.step();
    }
    self.owned.checksum_info.as_ref().unwrap().checksums.get (chunk as usize).cloned().unwrap_or (0)
  }
  fn debug_dump(&self, chunk: i64) ->BTreeMap<ExtendedTime <B>, u64>{
    self.test_lots();
    let mut result = BTreeMap::new();
    let info = self.owned.checksum_info.as_ref().unwrap();
    for (_, state) in self.owned.events.event_states.iter() {
      if (state.time.base.clone() - info.start.clone())/info.stride.clone() == chunk {
        if let Some (execution) = state.execution_state.as_ref() {
          result.insert (state.time.clone(), execution.checksum);
        }
      }
    }
    result
  }
  fn event_details (&self, time: & ExtendedTime <B>)->String {
    let mut result = String::new();
    let state = self.owned.events.event_states.get (&time .id).unwrap();
    use std::fmt::Write;
    write! (&mut result, "At {:?}:\n EventId: {:?}\n {:?}\n", time, state.schedule.as_ref().unwrap().event_id(), &state.execution_state).unwrap();
    result
  }
}

pub trait ChecksumTrait<B: Basics> {
  fn add_event_checksum(&mut self, _: u64, _: &B::Time) {
    unreachable!()
  }
}
impl<B: Basics> ChecksumTrait<B> for ChecksumInfo<B> {}

impl<B: Basics> ChecksumTrait<B> for ChecksumInfo<B>
  where B::Time: Sub<Output = B::Time> + Mul<i64, Output = B::Time> + Div<B::Time, Output = i64>
{
  fn add_event_checksum(&mut self, checksum: u64, time: &B::Time) {
    let chunk = (time.clone() - self.start.clone()) / self.stride.clone();
    assert!(chunk >= 0);
    while (self.checksums.len() as i64) <= chunk {
      self.checksums.push(0);
    }
    self.checksums[chunk as usize] = self.checksums[chunk as usize].wrapping_add(checksum);
  }
}
