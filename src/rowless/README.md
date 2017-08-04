# TimeSteward (under construction)

A game/simulation backend that automatically supports:
* [lockstep multiplayer](http://clintonbrennan.com/2013/12/lockstep-implementation-in-unity3d/) with reliable synchronization
* lag hiding
* non-blocking autosaves*
* parallel computation*
* and other features.

The main catch is that you have to write the physics within the TimeSteward model. Any physics is possible to write using TimeSteward, but it may not be easy to convert an existing simulation to use it.


## Short overview

TimeSteward has one core trick: It can change an event in the past, then cheaply update the present. It doesn't need to redo any computations that weren't dependent on that change.

Every time any event occurs, the TimeSteward records what data that event examined and modified. Thus, it can maintain a dependency graph between events. Ideally, all events only access or modify data within a small neighborhood, and dependencies don't propagate very fast. If these conditions are met, making a change to the recent past is very cheap.

This naturally supports lockstep multiplayer. Each client simply runs the simulation in real-time, handling local inputs immediately. When it receives input from other clients, it inserts that input into history *at the time the input was sent*. Thus, all clients ultimately end up with the same input history.

Individual clients can also speculatively simulate into the future, which lets them smooth over moments when very costly computations occur for only a short time.

Because TimeSteward retains old data, you can also cheaply take out a handle to a snapshot of the simulation state. You can trust that the snapshot won't change as the simulation continues. This allows, for instance, saving the snapshot to disk in a different thread*, without freezing the game while the user waits for the save to finish.


## Gotchas

In order to remain synchronized, all code executing within the simulation must be **deterministic**. (This doesn't apply to inputs which are manually shared between the clients.) Being deterministic means it can only depend on data from within the simulation. It cannot depend on other things, such as:

* The local system time
* System random number generation
* The floating-point implementation of the local processor
* The endianness of the local processor
* Whether data has been reloaded from a serialized version

In particular, you cannot use `f32`, `f64`, or `std::collections::HashMap`**.

TimeSteward provides some features to work around these limitations. It has a built-in deterministic PRNG. It will eventually also provide a deterministic alternative to HashMap and a deterministic plug-and-play replacement for f32/f64. (However, using floats may still be undesirable because floating-point emulation is much slower.)

TimeSteward also provides a convenient system for running test simulations synchronized over one or more computers. If synchronization fails, the system can report the exact event where the first failure occurred.


## Detailed design


### DataTimelines

All data that can *change over time* is stored in implementors of `trait DataTimeline`.

A DataTimeline is a **retroactive data structure**. You can insert operations in the present or past, and query it at any time in the present or past. The results of queries must depend only on the operations that exist at times earlier than the query – and not, for instance, on the order the operations were inserted. (You don't need to implement retroactive data structures yourself – TimeSteward has built-in types for common use cases. And if you do build your own, TimeSteward has features for testing that they behave correctly.)

A DataTimeline can also report *all* data existing as a specific time, as a snapshot. A snapshot taken at a specific time can be used to compute an exactly identical simulation, if the same user inputs are supplied after that. DataTimelines also have some other duties, which will be explained later.


DataTimelines can only change at discrete moments, and it is good to make those changes infrequent. If you want to represent, say, a moving object, the inner data should not just be the location of the object, but a representation of its trajectory over time:

```rust
struct Ball {
  // location at the last time the ball was modified
  location: [i64; 3],
  // velocity at the last time the ball was modified
  velocity: [i64; 3],
  // current constant acceleration – for instance, due to gravity or other forces
  acceleration: [i64; 3],
}
```

Thus, the data only needs to change when the forces on the ball change, such as when it runs into an object.

(In practice, the TimeSteward library provides implementations of a few trajectory types, so you may not have to implement this yourself. We will continue expanding the support libraries as development continues.)


### Rows and Columns

Each row has a deterministically-random RowId. RowIds can be generated by hashing (e.g. hash X and Y coordinates to get an ID), or you can request new unique RowId's from the TimeSteward PRNG. RowIds are 128 bits so that they will never accidentally collide with each other.

The Fields contain ALL data that can change over time, aside from user inputs. 


### Predictors and Events

If the data doesn't normally change over time, how do we know when to make things happen?

Imagine that a ball is moving towards a wall. From the current trajectory of the ball and the location of the wall, we can compute the time when the ball will hit the wall. This is the role of a **Predictor**.

A Predictor is essentially a Fn that can query the current DataTimelines, then report when an event will happen.

```rust
time_steward_predictor!{
  struct BallHitsWallPredictor,
  ...
  |&self, accessor| {
    let ball: &Ball = self.ball.query (accessor, ());
    ... // Examine various fields and compute the time when the ball hits the wall
    accessor.predict_at_time (time, BallHitsWallEvent::new (...));
  }
})
```

When a Predictor makes a query, the queried DataTimeline must keep track of this dependency. If there is any change to the result of the query, this invalidates the prediction, so the system reruns the Predictor to determine the new collision time.

If the prediction time arrives while the prediction is still valid, the **Event** happens. An Event is the only thing allowed to **insert** operations that change the data.

```rust
time_steward_event!{
  struct BallHitsWallEvent {ball_row_id: RowId, wall_row_id: RowId},
  ...
  |&self, mutator| {
    let mut ball: Ball = accessor.get::<Ball>(ball_row_id).clone();
    ... // Examine various fields and compute the new trajectory of the ball
    mutator.set::<Ball>(ball_id, ball);
  }
})
```

As shown above, Predictors and Events interact with the simulation through "accessor" and "mutator" objects. These objects are the way we track what queries and operations were made. Generally, it is an error for a Predictor or Event to get information by any means other than the accessor or mutator.



This system – Events automatically triggering Predictors, Predictors automatically creating Events – can implement a complete ongoing physics. The only thing missing is the way to add user input.

### FiatEvents

Events are the only thing that can change field data, but there are two ways Events can be created. One is to be predicted by a Predictor. The other is to be inserted from the outside by fiat. We call these FiatEvents. They usually represent user input, but they can also be based on the local time, instructions from the server, or other things. To keep simulations synchronized over a network, all FiatEvents, and *only* the FiatEvents, need to be shared between all clients.

### Ordering and DeterministicRandomIds

If two Events are scheduled to happen at the same time, one of them technically has to happen before the other. For the simulation to be deterministic, the order has to be deterministic as well.

We accomplish this by using a cryptographic hash function. Each Event is given a DeterministicRandomId – a unique 128 bit ID. Events happen in order by ID. For predicted events, each *Predictor* also needs its own unique id. For FiatEvents, the caller has to provide a unique random id. DeterministicRandomId can easily be generated from any type that implements Serialize:

```rust
for time in 0..50 {
  if the user is holding down the red button {
    steward.insert_fiat_event(
      time,
      DeterministicRandomId::new(&time),
      UserContinuesHoldingdownRedButtonEvent::new());
  }
}
```

A typical choice for FiatEvents would be to hash together a tuple of `(time, ID of user who gave the input, enum indicating the type of input)`.


### ExtendedTime

There's a special case when a Predictor predicts an event at the same time the Predictor was called. Imagine that a ball is going to collide with *two* walls at the same time, like in a corner. One of the events happens first, and the ball is deflected away from the one wall. This changes the ball, invalidating both predictions. However, the ball still needs to collide with the other wall. The second time the Predictor runs, it might generate a TimeId that comes *before* the TimeId of the first Event!

To deal with this, we still make the second Event happen at the same numerical time, but in a later **iteration**. This gives rise to the concept of an **ExtendedTime**, which is defined approximately as follows:

```rust
struct ExtendedTime {
  base: Time,
  iteration: u32,
  id: TimeId,
}
```

ExtendedTimes are lexicographically ordered by the fields listed above. TimeSteward users usually don't need to be aware of ExtendedTimes (just implement your Events in terms of regular time, and they will likely turn out fine). However, it *is* possible for TimeSteward users to examine ExtendedTimes, which can be useful for debugging and loop detection.


### TypeIds and serialization

All DataTimeline types, Event types, and Predictor types types also have random IDs. These IDs are simply one hard-coded u64 for each type. (Because there are fewer of them, they don't need to have as many bits to stay unique. Thanks to the [birthday problem](https://en.wikipedia.org/wiki/Birthday_problem), this would have a >1% chance of a collision with a mere 700 million types. I don't think we need to worry about this. 128 bit IDs are necessary for rows, because computers can generate billions of them easily, but this isn't the same situation.) The documentation provides a convenient way to generate these IDs. We could theoretically have these IDs be automatically generated from the type name and module path, which would make them unique, but hard-coding them helps keep serialization consistent from version to version of your program. (You wouldn't want savefiles to be incompatible just because you reorganized some modules.)


## Example

Coming soon...


## Optimizing TimeSteward simulations

Coming later...


## Keywords

TimeSteward uses **incremental processing** to be a **retroactive data structure**. The Predictor concept is a type of **reactive programming**. I didn't need these terms for the explanation, but I want them to appear in this document to attract people who are doing web searches for "reactive programming game physics" or similar.


## License

MIT


## Footnotes

*Not yet, but it is in the works.

**Even if you use a deterministic hasher, Hash implementations are endian-unsafe, which makes the ordering of the elements nondeterministic across systems. Also, the default Serialize and Deserialize impls for HashMap do not record the current capacity, which makes ordering of the elements nondeterministic under serialization.