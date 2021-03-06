//! A flat TimeSteward implementation that has good asymptotic performance for all operations.
//!
//! This implementation demonstrates the basic principles of how a flat TimeSteward can be efficient, without delving into serious optimizations or multithreading.
//!
//!


use ::{DeterministicRandomId, SiphashIdGenerator, RowId, FieldId, PredictorId, StewardRc, FieldRc,
       Accessor, Column, ExtendedTime, Basics, TimeSteward, TimeStewardFromConstants, FiatEventOperationError, ValidSince};
use implementation_support::common::{self, Filter, DynamicEventFn};
use std::collections::{HashMap, BTreeMap, HashSet};
use std::collections::hash_map::Entry;
use std::rc::Rc;
use std::cell::RefCell;
use std::ops::Drop;
use rand::Rng;
use std::cmp::max;
use implementation_support::insert_only;
use implementation_support::data_structures::{partially_persistent_nonindexed_set, BuildTrivialU64Hasher};

type SnapshotIdx = u64;

#[derive (Clone)]
struct Field<B: Basics> {
  data: FieldRc,
  last_change: ExtendedTime<B>,
  first_snapshot_not_updated: SnapshotIdx,
}
type SnapshotField<B> = (FieldRc, ExtendedTime<B>);


type FieldsMap<B> = HashMap<FieldId, Field<B>, BuildTrivialU64Hasher>;
type SnapshotsData<B> = BTreeMap<SnapshotIdx,
                                         Rc<insert_only::HashMap<FieldId, SnapshotField<B>, BuildTrivialU64Hasher>>>;

struct Fields<B: Basics> {
  field_states: FieldsMap<B>,
  changed_since_snapshots: SnapshotsData<B>,
}


#[derive (Clone)]
struct Prediction<B: Basics> {
  predictor_id: PredictorId,
  prediction_is_about_row_id: RowId,
  predictor_accessed: Vec<FieldId>,
  what_will_happen: Option<(ExtendedTime<B>, DynamicEvent<B>)>,
}

struct StewardShared<B: Basics> {
  settings: Settings<B>,
  constants: B::Constants,
  fields: RefCell<Fields<B>>,
}

struct StewardOwned<B: Basics> {
  last_event: Option<ExtendedTime<B>>,
  invalid_before: ValidSince<B::Time>,
  fiat_events: BTreeMap<ExtendedTime<B>, DynamicEvent<B>>,
  next_snapshot: SnapshotIdx,
  existent_fields: partially_persistent_nonindexed_set::Set<FieldId, BuildTrivialU64Hasher>,

  predictions_by_time: BTreeMap<ExtendedTime<B>, Rc<Prediction<B>>>,
  predictions_by_id: HashMap<(RowId, PredictorId), Rc<Prediction<B>>, BuildTrivialU64Hasher>,
  prediction_dependencies: HashMap<FieldId, HashSet<(RowId, PredictorId), BuildTrivialU64Hasher>, BuildTrivialU64Hasher>,
}

pub struct Steward<B: Basics> {
  owned: StewardOwned<B>,
  shared: Rc<StewardShared<B>>,
}
pub struct Snapshot<B: Basics> {
  now: B::Time,
  index: SnapshotIdx,
  field_states: Rc<insert_only::HashMap<FieldId, SnapshotField<B>, BuildTrivialU64Hasher>>,
  shared: Rc<StewardShared<B>>,
  num_fields: usize,
  field_ids: partially_persistent_nonindexed_set::Snapshot<FieldId>,
}
pub struct Mutator<'a, B: Basics> {
  generic: common::GenericMutator<B>,
  steward: &'a mut StewardOwned<B>,
  shared: &'a StewardShared<B>,
  fields: &'a mut Fields<B>,
  predictions_needed: HashSet<(RowId, PredictorId), BuildTrivialU64Hasher>,
}
pub struct PredictorAccessor<'a, B: Basics> {
  predictor_id: PredictorId,
  about_row_id: RowId,
  internal_now: ExtendedTime<B>,
  steward: RefCell<&'a mut StewardOwned<B>>,
  shared: &'a StewardShared<B>,
  fields: &'a Fields<B>,
  generic: common::GenericPredictorAccessor<B, DynamicEvent<B>>,
}

time_steward_common_dynamic_callback_structs! (Mutator, PredictorAccessor, DynamicEvent, DynamicPredictor, Settings);

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
          .field_states
          .get(&id)
          .and_then(|field| {
            if field.first_snapshot_not_updated > self.index {
              None
            } else {
              Some((field.data.clone(), field.last_change.clone()))
            }
          })

      })
      .map(|p| (&p.0, &p.1))
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
    self.fields.get(id)
  }
  fn constants(&self) -> &B::Constants {
    &self.shared.constants
  }
  time_steward_common_accessor_methods_for_mutator!(B);
}
impl<'a, B: Basics> PredictorAccessor<'a, B> {
  fn get_impl(&self, id: FieldId) -> Option<(&FieldRc, &ExtendedTime<B>)> {
    self.steward
      .borrow_mut()
      .prediction_dependencies
      .entry(id)
      .or_insert(HashSet::default())
      .insert((self.about_row_id, self.predictor_id));
    self.fields.get(id)
  }
}
impl<'a, B: Basics> ::Accessor for PredictorAccessor<'a, B> {
  type Basics = B;
  time_steward_common_accessor_methods_for_predictor_accessor!(B, get_impl);
  fn constants(&self) -> &B::Constants {
    &self.shared.constants
  }
  fn unsafe_now(&self) -> &B::Time {
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
    self.num_fields
  }
}

pub struct SnapshotIter<'a, B: Basics>(partially_persistent_nonindexed_set::SnapshotIter<'a,
                                                                                         FieldId>,
                                       &'a Snapshot<B>);
impl<'a, B: Basics> Iterator for SnapshotIter<'a, B> {
  type Item = (FieldId, (&'a FieldRc, &'a ExtendedTime<B>));
  fn next(&mut self) -> Option<Self::Item> {
    (self.0).next().map(|id| {
      (id,
       (self.1)
        .generic_data_and_extended_last_change(id)
        .expect("the snapshot thinks a FieldId exists when it doesn't"))
    })
  }
  fn size_hint(&self) -> (usize, Option<usize>) {
    (self.1.num_fields, Some(self.1.num_fields))
  }
}
impl<'a, B: Basics> IntoIterator for &'a Snapshot<B> {
  type Item = (FieldId, (&'a FieldRc, &'a ExtendedTime<B>));
  type IntoIter = SnapshotIter<'a, B>;
  fn into_iter(self) -> Self::IntoIter {
    SnapshotIter(self.field_ids.iter(), self)
  }
}

impl<B: Basics> Field<B> {
  fn update_snapshots(&self, my_id: FieldId, snapshots: &SnapshotsData<B>) {
    // Old snapshot are already "updated" with all nonexistent values
    for (index, snapshot_map) in snapshots.iter().rev() {
      if *index < self.first_snapshot_not_updated {
        break;
      }
      snapshot_map.get_default(my_id,
                               || Some((self.data.clone(), self.last_change.clone())));
    }
  }
}

impl<'a, B: Basics> ::Mutator for Mutator<'a, B> {
  fn set<C: Column>(&mut self, id: RowId, data: Option<C::FieldType>) {
    time_steward_common_mutator_set_prefix!(B, C, self, id, data);
    let field_id = FieldId {
      row_id: id,
      column_id: C::column_id(),
    };
    let old_value = self.fields.field_states.get(&field_id).cloned();
    let existence_changed = self.fields
      .set_opt::<C>(id, data, &self.generic.now, self.steward.next_snapshot);

    if existence_changed {
      self.shared.settings.predictors_by_column.get(&C::column_id()).map(|predictors| {
        for predictor in predictors {
          self.predictions_needed.insert((id, predictor.predictor_id));
        }
      });
      if old_value.is_none() {
        self.steward.existent_fields.insert(field_id);
      } else {
        self.steward.existent_fields.remove(field_id);
      }
    }
    if let Entry::Occupied(entry) = self.steward.prediction_dependencies.entry(field_id) {
      for prediction in entry.get() {
        self.predictions_needed.insert(prediction.clone());
      }
      entry.remove();
    }
  }
  time_steward_common_mutator_methods_for_mutator!(B);
}
impl<'a, B: Basics> Rng for Mutator<'a, B> {
  time_steward_common_rng_methods_for_mutator!(B);
}


impl<B: Basics> Fields<B> {
  fn get(&self, id: FieldId) -> Option<(&FieldRc, &ExtendedTime<B>)> {
    self.field_states.get(&id).map(|field| (&field.data, &field.last_change))
  }
  // returns true if the field changed from existing to nonexistent or vice versa
  fn set<C: Column>(&mut self,
                    id: RowId,
                    value: C::FieldType,
                    time: &ExtendedTime<B>,
                    next_snapshot: SnapshotIdx)
                    -> bool {
    let field = Field {
      data: StewardRc::new(value),
      last_change: time.clone(),
      first_snapshot_not_updated: next_snapshot,
    };
    let field_id = FieldId::new(id, C::column_id());
    match self.field_states
      .entry(field_id) {
      Entry::Occupied(mut entry) => {
        entry.get_mut().update_snapshots(field_id, &self.changed_since_snapshots);
        entry.insert(field);
        false
      }
      Entry::Vacant(entry) => {
        entry.insert(field);
        true
      }
    }
  }
  // returns true if the field changed from existing to nonexistent or vice versa
  fn remove<C: Column>(&mut self, id: RowId) -> bool {
    let field_id = FieldId::new(id, C::column_id());
    let removed = self.field_states
      .remove(&field_id);
    if let Some(value) = removed {
      value.update_snapshots(field_id, &self.changed_since_snapshots);
      return true;
    }
    false
  }
  // returns true if the field changed from existing to nonexistent or vice versa
  fn set_opt<C: Column>(&mut self,
                        id: RowId,
                        value_opt: Option<C::FieldType>,
                        time: &ExtendedTime<B>,
                        next_snapshot: SnapshotIdx)
                        -> bool {
    if let Some(value) = value_opt {
      self.set::<C>(id, value, time, next_snapshot)
    } else {
      self.remove::<C>(id)
    }
  }
}
impl<B: Basics> Steward<B> {
  fn next_event(&self) -> Option<(ExtendedTime<B>, DynamicEvent<B>)> {
    let first_fiat_event_iter = self.owned
      .fiat_events
      .iter()
      .map(|ev| (ev.0.clone(), ev.1.clone()))
      .take(1);
    let first_predicted_event_iter = self.owned
      .predictions_by_time
      .iter()
      .map(|pair| {
        (pair.0.clone(),
         pair.1
          .what_will_happen
          .as_ref()
          .expect("a prediction that predicted nothing was stored in predictions")
          .1
          .clone())
      })
      .take(1);
    let events_iter = first_fiat_event_iter.chain(first_predicted_event_iter);
    events_iter.min_by_key(|ev| ev.0.clone())
  }

  fn get_predictor(&self, predictor_id: PredictorId) -> &DynamicPredictor<B> {
    self.shared
      .settings
      .predictors_by_id
      .get(&predictor_id)
      .expect("somehow a PredictorId appeared with no associated predictor")
  }

  fn clear_prediction(&mut self, row_id: RowId, predictor_id: PredictorId) {
    if let Some(prediction) = self.owned.predictions_by_id.remove(&(row_id, predictor_id)) {
      for field_id in prediction.predictor_accessed.iter() {
        if let Entry::Occupied(mut entry) = self.owned
          .prediction_dependencies
          .entry(field_id.clone()) {
          entry.get_mut().remove(&(row_id, predictor_id));
          if entry.get().is_empty() {
            entry.remove();
          }
        }
      }
      if let Some((ref when, _)) = prediction.what_will_happen {
        self.owned.predictions_by_time.remove(when).expect("prediction records were inconsistent");
      }
    }
  }

  fn make_prediction(&mut self, row_id: RowId, predictor_id: PredictorId) {
    self.clear_prediction(row_id, predictor_id);
    if self.shared
      .fields
      .borrow()
      .field_states
      .get(&FieldId::new(row_id, self.get_predictor(predictor_id).column_id))
      .is_none() {
      return;
    }

    let now = self.owned
      .last_event
      .clone()
      .expect("how can we be calling a predictor when there are no fields yet?");
    let function = self.get_predictor(predictor_id).function.clone();
    let generic;
    {
      let field_ref = &*self.shared.fields.borrow();
      let mut pa = PredictorAccessor {
        predictor_id: predictor_id,
        about_row_id: row_id,
        internal_now: now,
        steward: RefCell::new(&mut self.owned),
        shared: &self.shared,
        fields: field_ref,
        generic: common::GenericPredictorAccessor::new(),
      };
      (function)(&mut pa, row_id);
      generic = pa.generic;
    }
    let (dependencies, hasher) = generic.dependencies.into_inner();
    let dependencies_hash = hasher.generate();
    let prediction = Rc::new(Prediction {
      predictor_id: predictor_id,
      prediction_is_about_row_id: row_id,
      predictor_accessed: dependencies,
      what_will_happen: generic.soonest_prediction.into_inner().and_then(|(event_base_time, event)| {
        common::next_extended_time_of_predicted_event(predictor_id,
                                                      row_id,
                                                      dependencies_hash,
                                                      event_base_time,
                                                      &self.owned
                                                        .last_event
                                                        .as_ref()
                                                        .expect("how can we be calling a \
                                                                 predictor when there are no \
                                                                 fields yet?"))
          .map(|event_time| (event_time, event))
      }),
    });
    self.owned.predictions_by_id.insert((row_id, predictor_id), prediction.clone());
    if let Some((ref time, _)) = prediction.what_will_happen {
      self.owned.predictions_by_time.insert(time.clone(), prediction.clone());
    }
  }

  fn execute_event(&mut self, event_time: ExtendedTime<B>, event: DynamicEvent<B>) {
    let predictions_needed;

    {
      let field_ref = &mut *self.shared.fields.borrow_mut();
      let mut mutator = Mutator {
        generic: common::GenericMutator::new(event_time.clone()),
        steward: &mut self.owned,
        shared: &self.shared,
        fields: field_ref,
        predictions_needed: HashSet::default(),
      };
      event(&mut mutator);
      predictions_needed = mutator.predictions_needed;
    }
    // if it was a fiat event, clean it up:
    self.owned.fiat_events.remove(&event_time);
    self.owned.last_event = Some(event_time);

    for (row_id, predictor_id) in predictions_needed {
      self.make_prediction(row_id, predictor_id);
    }
  }

  fn update_until_beginning_of(&mut self, target_time: &B::Time) {
    while let Some(ev) = self.next_event().filter(|ev| ev.0.base < *target_time) {
      let (event_time, event) = ev;
      self.execute_event(event_time, event);
    }
  }
}


impl<B: Basics> TimeSteward for Steward<B> {
  type Basics = B;
  type Snapshot = Snapshot<B>;

  fn valid_since(&self) -> ValidSince<B::Time> {
    max(self.owned.invalid_before.clone(),
        match self.owned.last_event {
          None => ValidSince::TheBeginning,
          Some(ref time) => ValidSince::After(time.base.clone()),
        })
  }

  fn insert_fiat_event<E: ::Event<Basics = B>>(&mut self,
                                               time: B::Time,
                                               id: DeterministicRandomId,
                                               event: E)
                                               -> Result<(), FiatEventOperationError> {
    time_steward_common_insert_fiat_event_prefix!(B, self, time, E);
    match self.owned.fiat_events.insert(common::extended_time_of_fiat_event(time, id),
                                        StewardRc::new(DynamicEventFn::new(event))) {
      None => Ok(()),
      Some(_) => Err(FiatEventOperationError::InvalidInput),
    }
  }

  fn remove_fiat_event(&mut self,
                       time: &B::Time,
                       id: DeterministicRandomId)
                       -> Result<(), FiatEventOperationError> {
    if self.valid_since() > *time {
      return Err(FiatEventOperationError::InvalidTime);
    }
    match self.owned.fiat_events.remove(&common::extended_time_of_fiat_event(time.clone(), id)) {
      None => Err(FiatEventOperationError::InvalidInput),
      Some(_) => Ok(()),
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
      .or_insert(Rc::new(insert_only::HashMap::default()))
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

impl<B: Basics> TimeStewardFromConstants for Steward<B> {
  fn from_constants(constants: B::Constants) -> Self {
    Steward {
      owned: StewardOwned {
        last_event: None,
        invalid_before: ValidSince::TheBeginning,
        fiat_events: BTreeMap::new(),
        next_snapshot: 0,
        existent_fields: partially_persistent_nonindexed_set::Set::default(),
        predictions_by_time: BTreeMap::new(),
        predictions_by_id: HashMap::default(),
        prediction_dependencies: HashMap::default(),
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
    let mut result = Self::from_constants (snapshot.constants().clone());
    result.owned.invalid_before = ValidSince::Before(snapshot.now().clone());
    let mut predictions_needed = HashSet::new();
    result.shared.fields.borrow_mut().field_states = snapshot.into_iter()
      .map(|(id, stuff)| {
        if match result.owned.last_event {
          None => true,
          Some(ref time) => stuff.1 > time,
        } {
          result.owned.last_event = Some(stuff.1.clone());
        }
        result.shared.settings.predictors_by_column.get(&id.column_id).map(|predictors| {
          for predictor in predictors {
            predictions_needed.insert((id.row_id, predictor.predictor_id));
          }
        });
        (id,
         Field {
          data: stuff.0.clone(),
          last_change: stuff.1.clone(),
          first_snapshot_not_updated: 0,
        })
      })
      .collect();
    for (row_id, predictor_id) in predictions_needed {
      result.make_prediction(row_id, predictor_id);
    }

    result
  }
}

impl<B: Basics> ::IncrementalTimeSteward for Steward<B> {
  fn step(&mut self) {
    if let Some(ev) = self.next_event() {
      let (event_time, event) = ev;
      self.execute_event(event_time, event);
    }
  }
  fn updated_until_before(&self) -> Option<B::Time> {
    self.next_event().map(|(time, _)| time.base)
  }
}
impl<B: Basics> ::CanonicalTimeSteward for Steward<B> {}
