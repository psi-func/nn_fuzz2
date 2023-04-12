use core::mem::size_of;
use libafl::prelude::{
    tuple_list, tuple_list_type, Error, HasBytesVec, HasMaxSize, HasRand, MutationResult, Mutator,
    MutatorsTuple, Named, Rand, buffer_self_copy, buffer_set
};
use std::fmt::{self, Debug};
use std::marker::PhantomData;

pub struct NnMutator<I, MT, S>
where
    MT: MutatorsTuple<I, S>,
{
    hotbytes: Vec<u32>,
    mutations: MT,
    phantom: PhantomData<(I, S)>,
}

impl<I, MT, S> Debug for NnMutator<I, MT, S>
where
    S: HasRand,
    MT: MutatorsTuple<I, S>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NnMutator with {} mutations for Input type {}",
            self.mutations.len(),
            core::any::type_name::<I>()
        )
    }
}

impl<I, MT, S> NnMutator<I, MT, S>
where
    MT: MutatorsTuple<I, S>,
{   
    #[must_use]
    pub fn new(mutations: MT) -> Self {
        Self {
            hotbytes: Vec::default(),
            mutations,
            phantom: PhantomData,
        }
    }

    #[allow(dead_code)]
    pub fn hotbytes(&self) -> &Vec<u32> {
        &self.hotbytes
    }

    pub fn hotbytes_mut(&mut self) -> &mut Vec<u32> {
        &mut self.hotbytes
    }

    pub fn mutations(&self) -> &MT {
        &self.mutations
    }

    pub fn mutations_mut(&mut self) -> &mut MT {
        &mut self.mutations
    }
}

impl<I, MT, S> Mutator<I, S> for NnMutator<I, MT, S>
where
    MT: MutatorsTuple<I, S>,
    S: HasRand,
    I: HasBytesVec,
{
    fn mutate(
        &mut self,
        state: &mut S,
        input: &mut I,
        _stage_idx: i32,
    ) -> Result<MutationResult, Error> {
        let mut r = MutationResult::Skipped;
        let idx = *state.rand_mut().choose(self.hotbytes.as_slice()) as usize;
        if idx < input.bytes().len() {
            let index = state.rand_mut().below(self.mutations().len() as u64);
            let outcome =
                self.mutations_mut()
                    .get_and_mutate(index.into(), state, input, idx as i32)?;
            if outcome == MutationResult::Mutated {
                r = MutationResult::Mutated;
            }
        }
        Ok(r)
    }
}

pub type RlMutationTuple = tuple_list_type!(
    AssignByteMutator,
    BytesDeleteMutator,
    BytesInsertMutator,
    ByteAddMutator,
    WordAddMutator,
    DwordAddMutator,
    QwordAddMutator,
    ByteInterestingMutator,
    WordInterestingMutator,
    DwordInterestingMutator,
);

pub fn rl_mutations() -> RlMutationTuple {
    tuple_list!(
        AssignByteMutator::new(),
        BytesDeleteMutator::new(),
        BytesInsertMutator::new(),
        ByteAddMutator::new(),
        WordAddMutator::new(),
        DwordAddMutator::new(),
        QwordAddMutator::new(),
        ByteInterestingMutator::new(),
        WordInterestingMutator::new(),
        DwordInterestingMutator::new(),
    )
}

const ARITH_MAX: u64 = 35;

const INTERESTING_8: [i8; 9] = [-128, -1, 0, 1, 16, 32, 64, 100, 127];
/// Interesting 16-bit values from AFL
const INTERESTING_16: [i16; 19] = [
    -128, -1, 0, 1, 16, 32, 64, 100, 127, -32768, -129, 128, 255, 256, 512, 1000, 1024, 4096, 32767,
];
/// Interesting 32-bit values from AFL
const INTERESTING_32: [i32; 27] = [
    -128,
    -1,
    0,
    1,
    16,
    32,
    64,
    100,
    127,
    -32768,
    -129,
    128,
    255,
    256,
    512,
    1000,
    1024,
    4096,
    32767,
    -2_147_483_648,
    -100_663_046,
    -32769,
    32768,
    65535,
    65536,
    100_663_045,
    2_147_483_647,
];

/// # Mutators
///
///
macro_rules! add_mutator_impl {
    ($name: ident, $size: ty) => {
        #[derive(Default, Debug)]
        pub struct $name;

        #[allow(trivial_numeric_casts)]
        impl<I, S> Mutator<I, S> for $name
        where
            S: HasRand,
            I: HasBytesVec,
        {
            fn mutate(
                &mut self,
                state: &mut S,
                input: &mut I,
                hotbyte_idx: i32,
            ) -> Result<MutationResult, Error> {
                let upper_bound = hotbyte_idx as usize + size_of::<$size>() - 1;
                if upper_bound >= input.bytes().len() {
                    Ok(MutationResult::Skipped)
                } else {
                    let (index, bytes) = input
                        .bytes()
                        .windows(size_of::<$size>())
                        .enumerate()
                        .nth(hotbyte_idx as usize)
                        .unwrap();
                    let val = <$size>::from_ne_bytes(bytes.try_into().unwrap());

                    // mutate
                    let num = 1 + state.rand_mut().below(ARITH_MAX) as $size;
                    let new_val = match state.rand_mut().below(4) {
                        0 => val.wrapping_add(num),
                        1 => val.wrapping_sub(num),
                        2 => val.swap_bytes().wrapping_add(num).swap_bytes(),
                        _ => val.swap_bytes().wrapping_sub(num).swap_bytes(),
                    };
                    // set bytes to mutated value
                    let new_bytes = &mut input.bytes_mut()[index..index + size_of::<$size>()];
                    new_bytes.copy_from_slice(&new_val.to_ne_bytes());
                    Ok(MutationResult::Mutated)
                }
            }
        }

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self
            }
        }

        impl Named for $name {
            fn name(&self) -> &str {
                stringify!($name)
            }
        }
    };
}

add_mutator_impl!(ByteAddMutator, u8);
add_mutator_impl!(WordAddMutator, u16);
add_mutator_impl!(DwordAddMutator, u32);
add_mutator_impl!(QwordAddMutator, u64);

macro_rules! interesting_mutator_impl {
    ($name: ident, $size: ty, $interesting: ident) => {
        /// Inserts an interesting value at a random place in the input vector
        #[derive(Default, Debug)]
        pub struct $name;

        impl<I, S> Mutator<I, S> for $name
        where
            S: HasRand,
            I: HasBytesVec,
        {
            #[allow(clippy::cast_sign_loss)]
            fn mutate(
                &mut self,
                state: &mut S,
                input: &mut I,
                hotbyte_idx: i32,
            ) -> Result<MutationResult, Error> {
                if input.bytes().len() < size_of::<$size>() {
                    Ok(MutationResult::Skipped)
                } else {
                    let bytes = input.bytes_mut();
                    let upper_bound = (bytes.len() + 1 - size_of::<$size>()) as u64;
                    if hotbyte_idx as u64 >= upper_bound {
                        return Ok(MutationResult::Skipped);
                    }
                    let idx = hotbyte_idx as usize;
                    let val = *state.rand_mut().choose(&$interesting) as $size;
                    let new_bytes = match state.rand_mut().choose(&[0, 1]) {
                        0 => val.to_be_bytes(),
                        _ => val.to_le_bytes(),
                    };
                    bytes[idx..idx + size_of::<$size>()].copy_from_slice(&new_bytes);
                    Ok(MutationResult::Mutated)
                }
            }
        }

        impl Named for $name {
            fn name(&self) -> &str {
                stringify!($name)
            }
        }

        impl $name {
            /// Creates a new [`$name`].
            #[must_use]
            pub fn new() -> Self {
                Self
            }
        }
    };
}

interesting_mutator_impl!(ByteInterestingMutator, u8, INTERESTING_8);
interesting_mutator_impl!(WordInterestingMutator, u16, INTERESTING_16);
interesting_mutator_impl!(DwordInterestingMutator, u32, INTERESTING_32);


#[derive(Default, Debug)]
pub struct AssignByteMutator;

impl AssignByteMutator {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Named for AssignByteMutator {
    fn name(&self) -> &str {
        "AssignByteMutator"
    }
}

impl<I, S> Mutator<I, S> for AssignByteMutator
where
    S: HasRand,
    I: HasBytesVec,
{
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    fn mutate(
        &mut self,
        state: &mut S,
        input: &mut I,
        hotbyte_idx: i32,
    ) -> Result<MutationResult, Error> {
        let upper_bound = hotbyte_idx as usize;
        if upper_bound >= input.bytes().len() {
            Ok(MutationResult::Skipped)
        } else {
            let byte = input
                .bytes_mut()
                .iter_mut()
                .nth(hotbyte_idx as usize)
                .unwrap();
            *byte ^= 1 + state.rand_mut().below(254) as u8;
            Ok(MutationResult::Mutated)
        }
    }
}

#[derive(Default, Debug)]
pub struct BytesDeleteMutator;

impl<I, S> Mutator<I, S> for BytesDeleteMutator
where
    S: HasRand,
    I: HasBytesVec,
{
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    fn mutate(
        &mut self,
        state: &mut S,
        input: &mut I,
        hotbyte_idx: i32,
    ) -> Result<MutationResult, Error> {
        let size = input.bytes().len();
        if size <= 2 {
            return Ok(MutationResult::Skipped);
        }

        let off = hotbyte_idx as usize;
        let len = state.rand_mut().below((size - off) as u64) as usize;
        input.bytes_mut().drain(off..off + len);

        Ok(MutationResult::Mutated)
    }
}

impl Named for BytesDeleteMutator {
    fn name(&self) -> &str {
        "BytesDeleteMutator"
    }
}

impl BytesDeleteMutator {
    /// Creates a new [`BytesDeleteMutator`].
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[derive(Default, Debug)]
pub struct BytesInsertMutator;

impl<I, S> Mutator<I, S> for BytesInsertMutator
where
    S: HasRand + HasMaxSize,
    I: HasBytesVec,
{
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    fn mutate(
        &mut self,
        state: &mut S,
        input: &mut I,
        hotbyte_idx: i32,
    ) -> Result<MutationResult, Error> {
        let max_size = state.max_size();
        let size = input.bytes().len();
        if size == 0 || hotbyte_idx as usize > size {
            return Ok(MutationResult::Skipped);
        }
        let off = hotbyte_idx as usize;
        let mut len = 1 + state.rand_mut().below(16) as usize;

        if size + len > max_size {
            if max_size > size {
                len = max_size - size;
            } else {
                return Ok(MutationResult::Skipped);
            }
        }

        let val = input.bytes()[state.rand_mut().below(size as u64) as usize];

        input.bytes_mut().resize(size + len, 0);
        buffer_self_copy(input.bytes_mut(), off, off + len, size - off);
        buffer_set(input.bytes_mut(), off, len, val);

        Ok(MutationResult::Mutated)
    }
}

impl Named for BytesInsertMutator {
    fn name(&self) -> &str {
        "BytesInsertMutator"
    }
}

impl BytesInsertMutator {
    /// Creates a new [`BytesInsertMutator`].
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}
