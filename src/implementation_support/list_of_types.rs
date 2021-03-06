use std::marker::PhantomData;
use {Column, Event, Predictor, Basics};

//pub trait Contains <T> {}
pub trait AmI <T> {fn am_i()->bool;}
impl <T, U> AmI <T> for U {default fn am_i()->bool {false}}
impl <T> AmI <T> for T {fn am_i()->bool {true}}

macro_rules! type_list_definitions {
($module: ident, $Trait: ident, $IdType: ident, $get_id: ident) => {
pub mod $module {
use std::any::Any;
use std::marker::PhantomData;
use {$Trait,$IdType};

pub type Id = $IdType;
pub use $Trait as Trait;
pub fn get_id <T: $Trait>()->Id {T::$get_id()}

enum Void {}
pub struct Item <T: $Trait>(PhantomData <T>, Void);
pub trait User {
  fn apply<T: $Trait>(&mut self);
}
pub trait List: Any {
  fn apply<U: User>(user: &mut U);
}
impl<T: Any> List for T {
  #[inline(always)]
  default fn apply<U: User>(_: &mut U) {}
}
impl<T: $Trait> List for Item <T> {
  #[inline(always)]
  fn apply<U: User>(user: &mut U) {
    user.apply::<T>();
  }
}
//impl <T: $Trait> super::Contains <T> for Item <T> {}

tuple_impls! (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, T24, T25, T26, T27, T28, T29, T30, T31);
}
};

($module: ident, $Trait: ident <B>, $IdType: ident, $get_id: ident) => {
pub mod $module {
use std::any::Any;
use std::marker::PhantomData;
use {$Trait,$IdType, Basics};

pub type Id = $IdType;
pub use $Trait as Trait;
pub fn get_id <T: $Trait>()->Id {T::$get_id()}

enum Void {}
pub struct Item <T: $Trait>(PhantomData <T>, Void);
pub trait User <B: Basics> {
  fn apply<T: $Trait <Basics = B>>(&mut self);
}
pub trait List <B: Basics>: Any {
  fn apply<U: User <B>>(user: &mut U);
}
impl<B: Basics, T: Any> List <B> for T {
  #[inline]
  default fn apply<U: User <B>>(_: &mut U) {}
}
impl<B: Basics, T: $Trait<Basics = B>> List <B> for Item <T> {
  #[inline]
  fn apply<U: User <B>>(user: &mut U) {
    user.apply::<T>();
  }
}
//impl <T: $Trait> super::Contains <T> for Item <T> {}

tuple_impls! (B: T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, T24, T25, T26, T27, T28, T29, T30, T31);
}
};
}
macro_rules! tuple_impls {
  ($TL: ident $(, $T: ident)*) => {
    impl<$($T,)* $TL> List for ($($T,)* $TL,)
      where $($T: List,)* $TL: List
    {
      #[inline(always)]
      fn apply <U: User> (user: &mut U) {
        $($T::apply(user);)*
        $TL::apply(user);
      }
    }
    tuple_impls! ($($T),*);
  };
  () => {};
  (B: $TL: ident $(, $T: ident)*) => {
    impl<B: Basics, $($T,)* $TL> List <B> for ($($T,)* $TL,)
      where $($T: List <B>,)* $TL: List <B>
    {
      #[inline(always)]
      fn apply <U: User <B>> (user: &mut U) {
        $($T::apply(user);)*
        $TL::apply(user);
      }
    }
    tuple_impls! (B: $($T),*);
  };
  (B:) => {};
}
/*
macro_rules! contains_tuple_impls {
  (@0 [$L0: ident $($L: ident)*][$R0: ident $($R: ident)*]) => {
    contains_tuple_impls! (@0 [$($L)*] [$($R)*]);
    contains_tuple_impls! (@1 [$L0 $($L)*] [$R0 $($R)*]);
  };
  (@0 [][]) => {};
  (@1 [$L0: ident $($L: ident)*][$($R: ident)*]) => {
    contains_tuple_impls! (@1 [$($L)*] [$($R)*]);
    
    impl<Contained, $($R,)*> Contains <Contained> for ($($R,)*)
      where $L0: Contains <Contained> {}
  };
  (@1 [][$($R: ident)*]) => {};
  ($($T:ident),*) => {
    contains_tuple_impls! (@0 [$($T)*] [$($T)*]);
  };
}

macro_rules! pair_null_impls {
($module0: ident $module1: ident) => {
impl<T: $module0::Trait> $module1::List for $module0::Item <T> {
  #[inline]
  fn apply<U: $module1::User>(_: &mut U) {}
}
impl<T: $module1::Trait> $module0::List for $module1::Item <T> {
  #[inline]
  fn apply<U: $module0::User>(_: &mut U) {}
}
};
}
macro_rules! all_null_impls {
($info0:tt $($info:tt)*) => {
  $(pair_null_impls! ($info0 $info);)*
  all_null_impls! ($($info)*);
};
() => {};
}
*/
macro_rules! all_list_definitions {
($([$($info:tt)*])*) => {
  $(type_list_definitions! ($($info)*);)*
  //all_null_impls! ($([$($info)*])*);
};
}

// Today I Learned that macro hygiene is not applied to type parameter lists
//
// macro_rules! escalate {
// ([$first:tt $($whatever:tt)*] $($T: ident)*) => {escalate! ([$($whatever)*] foo $($T)*);};
// ([] $($T: ident)*) => {tuple_impls! ($($T),*);};
// }
// escalate! ([!!!!!!!! !!!!!!!! !!!!!!!! !!!!!!!!]);
//

all_list_definitions! (
  [column_list, Column, ColumnId, column_id]
  [event_list, Event <B>, EventId, event_id]
  [predictor_list, Predictor <B>, PredictorId, predictor_id]
);
// all_null_impls! (column_list event_list predictor_list);
//contains_tuple_impls! (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, T24, T25, T26, T27, T28, T29, T30, T31);

pub use self::column_list::List as ColumnList;
pub use self::column_list::Item as ColumnType;
pub use self::event_list::List as EventList;
pub use self::event_list::Item as EventType;
pub use self::predictor_list::List as PredictorList;
pub use self::predictor_list::Item as PredictorType;

/*
#[macro_export]
macro_rules! time_steward_make_function_table_type {
  ($module: ident, struct $Struct: ident, fn $memoized_function: ident, fn $function: ident <$T: ident: $Trait: ident [$($trait_parameters:tt)*]  $(, [$Parameter: ident $($bounds:tt)*])*> ($($argument_name: ident: $argument_type:ty),*)->$return_type:ty) => {
  
  
pub struct $Struct <$($Parameter $($bounds)*),*> (HashMap<$crate::implementation_support::list_of_types::$module::Id, fn($($argument_name: $argument_type),*)-> $return_type>);
impl<$($Parameter $($bounds)*),*> $crate::implementation_support::list_of_types::$module::User <$($trait_parameters)*> for $Struct<$($Parameter),*> {
  fn apply<$T: $Trait>(&mut self) {
    self.0.insert($crate::implementation_support::list_of_types::$module::get_id:: <$T>(), $function::<$T $(, $Parameter)*>);
  }
}
impl<$($Parameter $($bounds)*),*> $Struct<$($Parameter),*> {
  pub fn new <L: $crate::implementation_support::list_of_types::$module::List<$($trait_parameters)*> >()->$Struct<$($Parameter),*> {
    let mut result = $Struct (::std::collections::HashMap::new());
    L::apply (&mut result);
    result
  }
  pub fn get (&self, id: $crate::implementation_support::list_of_types::$module::Id)->fn ($($argument_type),*)->$return_type {
    *(self.0.get (&id).expect ("Type missing from function table; did you forget to list it in Basics::IncludedTypes?"))
  }
  pub fn call (&self, id: $crate::implementation_support::list_of_types::$module::Id $(, $argument_name: $argument_type)*)->$return_type {
    self.get (id)($($argument_name),*)
  }
}

#[allow (unused_imports)]
pub fn $memoized_function <L: $crate::implementation_support::list_of_types::$module::List <$($trait_parameters)*> $(, $Parameter $($bounds)*)*> (id: $crate::implementation_support::list_of_types::$module::Id $(, $argument_name: $argument_type)*)-> $return_type where L: ::std::any::Any $(, $Parameter: ::std::any::Any)* {
  use std::any::{Any, TypeId};
  use std::cell::RefCell;
  use std::collections::HashMap;
  thread_local! {static TABLE: RefCell<HashMap <(TypeId, $(time_steward_make_function_table_type ! (replace_with_typeid $Parameter)),*), Box <Any>>> = RefCell::new (HashMap::new());}
  let function = TABLE.with (| table | {
    table.borrow_mut().entry ((TypeId::of::<L>(), $(TypeId::of::<$Parameter>()),*)).or_insert (Box::new ($Struct ::<$($Parameter),*>::new::<L>())).downcast_ref::<$Struct <$($Parameter),*>>().unwrap().get (id)
  });
  function ($($argument_name),*)
}


};

}
*/

#[macro_export]
macro_rules! time_steward_dynamic_fn {
  (pub fn $($rest:tt)*) => { time_steward_dynamic_fn! (@privacy $($rest)* => [pub]); };
  (fn $($rest:tt)*) => { time_steward_dynamic_fn! (@privacy $($rest)* => []); };

  (@privacy $name: ident <$B: ident: Basics $(, [$Parameter: ident: Any $($bounds:tt)*])*> $($rest:tt)*) => { time_steward_dynamic_fn! (@parameters $B $($rest)* $name [[$B: Basics] $([$Parameter: Any $($bounds)*])*]); };

  (@parameters $B: ident ($id: ident: ColumnId of <$T: ident: Column> $(, $argument_name: ident: $argument_type:ty)*) $($rest:tt)*) => {
    time_steward_dynamic_fn! (@arguments $($rest)*
      [$B, $id, ColumnId, $T, column_list] [Column] [ColumnList] [column_list::User]
      [$id: ColumnId $(, $argument_name: $argument_type)*]
    );};
  (@parameters $B: ident ($id: ident: EventId of <$T: ident: Event <Basics = $B2: ident>> $(, $argument_name: ident: $argument_type:ty)*) $($rest:tt)*) => {
    time_steward_dynamic_fn! (@arguments $($rest)*
      [$B, $id, EventId, $T, event_list] [Event <Basics = $B>] [EventList <$B>] [event_list::User <$B>]
      [$id: EventId $(, $argument_name: $argument_type)*]
    );};
  (@parameters $B: ident ($id: ident: PredictorId of <$T: ident: Predictor <Basics = $B2: ident>> $(, $argument_name: ident: $argument_type:ty)*) $($rest:tt)*) => {
    time_steward_dynamic_fn! (@arguments $($rest)*
      [$B, $id, PredictorId, $T, predictor_list] [Predictor <Basics = $B>] [PredictorList <$B>] [predictor_list::User <$B>]
      [$id: PredictorId $(, $argument_name: $argument_type)*]
    );};

  (@arguments -> $return_type: ty [where $($clause:tt)*] {$($body: tt)*} => $($rest:tt)*) => { time_steward_dynamic_fn! (@complete $($rest)* $return_type [where $($clause)*] [$($body)*]); };
  (@arguments -> $return_type: ty {$($body: tt)*} => $($rest:tt)*) => { time_steward_dynamic_fn! (@complete $($rest)* $return_type [] [$($body)*]);};

  (@replace_with_typeid $Parameter: ident) => {TypeId};

  (@complete
    [$($privacy:tt)*]
    $name: ident [$([$Parameter: ident $($bounds:tt)*])*]
    [$B: ident, $id: ident, $Id:ty, $T: ident, $module: ident] [$($Trait:tt)*] [$($ List:tt)*] [$($ User:tt)*]
    [$($argument_name: ident: $argument_type:ty),*] $return_type:ty
    [$($where_clause:tt)*] [$($body:tt)*]
    ) => {

    $($privacy)* fn $name
      <$($Parameter $($bounds)*),*>
      ($($argument_name: $argument_type),*)
      ->$return_type
      $($where_clause)*
    {
      #[allow (unused_variables)]
      fn inner <$T: $($Trait)* $(, $Parameter $($bounds)*)*>
        ($($argument_name: $argument_type),*)
        ->$return_type
        $($where_clause)*
        {$($body)*}

      struct Table <$($Parameter $($bounds)*),*> (HashMap<$Id, fn($($argument_name: $argument_type),*)-> $return_type, $crate::implementation_support::data_structures::BuildTrivialU64Hasher>,::std::marker::PhantomData <($($Parameter),*)>);
      impl<$($Parameter $($bounds)*),*> $crate::implementation_support::list_of_types::$($User)* for Table <$($Parameter),*> {
        fn apply<T: $($Trait)*>(&mut self) {
          self.0.insert($crate::implementation_support::list_of_types::$module::get_id::<T>(), inner::<T $(, $Parameter)*>);
        }
      }
      impl<$($Parameter $($bounds)*),*> Table <$($Parameter),*> {
        fn new()-> Table <$($Parameter),*> {
          $crate::implementation_support::list_of_types::audit_basics::<$B>();
          let mut result = Table (::std::collections::HashMap::default(),::std::marker::PhantomData);
          <$B::IncludedTypes as $crate::implementation_support::list_of_types::$($List)*>::apply (&mut result);
          result
        }
        pub fn get (&self, id: $Id)->fn ($($argument_type),*)->$return_type {
          *(self.0.get (&id).unwrap_or_else (|| panic! ("Type with {:?} missing from function table; did you forget to list it in Basics::IncludedTypes?", id)))
        }
      }

      use std::any::{Any, TypeId};
      use std::cell::RefCell;
      use std::collections::HashMap;
      thread_local! {static TABLE: RefCell<HashMap <($(time_steward_dynamic_fn! (@replace_with_typeid $Parameter)),*), Box <Any>>> = RefCell::new (HashMap::new());}
      let function = TABLE.with (| table | {
        table.borrow_mut().entry (($(TypeId::of::<$Parameter>()),*)).or_insert (Box::new (Table::<$($Parameter),*>::new())).downcast_ref::<Table <$($Parameter),*>>().unwrap().get ($id)
      });
      function ($($argument_name),*)
    }
  }
}

fn id_suggestions()->String {
  use rand::Rng;
  if let Ok (mut rng) = ::rand::os::OsRng::new() {
    format! ("Try using some of these newly generated IDs instead:\nColumnId(0x{:016x})\nColumnId(0x{:016x})\nColumnId(0x{:016x})\nEventId(0x{:016x})\nEventId(0x{:016x})\nEventId(0x{:016x})\nPredictorId(0x{:016x})\nPredictorId(0x{:016x})\nPredictorId(0x{:016x})", rng.gen::<u64>(), rng.gen::<u64>(), rng.gen::<u64>(), rng.gen::<u64>(), rng.gen::<u64>(), rng.gen::<u64>(), rng.gen::<u64>(), rng.gen::<u64>(), rng.gen::<u64>())
  } else {String::new()}
}

pub fn audit_basics<Q: Basics>() {
  use std::collections::HashMap;

  struct Table<B: Basics>(HashMap<u64, u32>, PhantomData<B>);
  impl<B: Basics> Table<B> {
    fn insert_id(&mut self, id: u64, traitidx: u32) {
      if id <= 9000 {
        panic! ("This ID isn't random enough: 0x{:016x}\n{}", id, id_suggestions());
      }
      if self.0.insert(id, traitidx).is_some() {
        panic! ("Multiple IncludedTypes had the same id: 0x{:016x}\n{}", id, id_suggestions());
      }
    }
  }

  impl<B: Basics> column_list::User for Table<B> {
    #[inline (always)]
    fn apply<T: Column>(&mut self) {
      self.insert_id(T::column_id().0, 0);
    }
  }

  impl<B: Basics> event_list::User<B> for Table<B> {
    #[inline (always)]
    fn apply<T: Event>(&mut self) {
      self.insert_id(T::event_id().0, 1);
    }
  }

  impl<B: Basics> predictor_list::User<B> for Table<B> {
    fn apply<T: Predictor>(&mut self) {
      self.insert_id(T::predictor_id().0, 2);
      match self.0.get(&T::WatchedColumn::column_id().0) {
        Some(&0) => (),
        _=>panic! ("Predictor 0x{:016x} corresponds to column 0x{:016x}, but no such column is listed", T::predictor_id().0, T::WatchedColumn::column_id().0),
      }
    }
  }
  let mut checker = Table::<Q>(::std::collections::HashMap::new(), PhantomData);
  <Q::IncludedTypes as ColumnList>::apply(&mut checker);
  <Q::IncludedTypes as EventList<Q>>::apply(&mut checker);
  <Q::IncludedTypes as PredictorList<Q>>::apply(&mut checker);
}

#[inline (always)]
pub fn contains_column<B: Basics, C: Column>()->bool {
  struct Checker <B: Basics, C: Column>(bool, PhantomData<(B, C)>);
  
  impl<B: Basics, C: Column> column_list::User for Checker <B, C> {
    #[inline (always)]
    fn apply<T: Column>(&mut self) {
      if <T as AmI<C>>::am_i() {self.0 = true;}
    }
  }

  let mut checker = Checker::<B, C>(false, PhantomData);
  <B::IncludedTypes as ColumnList>::apply(&mut checker);
  checker.0
}

#[inline (always)]
pub fn contains_event <B: Basics, E: Event>()->bool {
  struct Checker <B: Basics, E: Event>(bool, PhantomData<(B, E)>);
  
  impl<B: Basics, E: Event> event_list::User <B> for Checker <B, E> {
    #[inline (always)]
    fn apply<T: Event>(&mut self) {
      if <T as AmI<E>>::am_i() {self.0 = true;}
    }
  }

  let mut checker = Checker::<B, E>(false, PhantomData);
  <B::IncludedTypes as EventList <B>>::apply(&mut checker);
  checker.0
}

#[inline (always)]
pub fn contains_predictor <B: Basics, P: Predictor>()->bool {
  struct Checker <B: Basics, P: Predictor>(bool, PhantomData<(B, P)>);
  
  impl<B: Basics, P: Predictor> predictor_list::User <B> for Checker <B, P> {
    #[inline (always)]
    fn apply<T: Predictor>(&mut self) {
      if <T as AmI<P>>::am_i() {self.0 = true;}
    }
  }

  let mut checker = Checker::<B, P>(false, PhantomData);
  <B::IncludedTypes as PredictorList <B>>::apply(&mut checker);
  checker.0
}

pub fn assert_contains_column<B: Basics, T: Column>() {
  assert! (contains_column::<B, T>(), "Type with {:?} missing from Basics::IncludedTypes", T::column_id());
}

pub fn assert_contains_event <B: Basics, T: Event>() {
  assert! (contains_event::<B, T>(), "Type with {:?} missing from Basics::IncludedTypes", T::event_id());
}

pub fn assert_contains_predictor <B: Basics, T: Predictor>() {
  assert! (contains_predictor::<B, T>(), "Type with {:?} missing from Basics::IncludedTypes", T::predictor_id());
}

