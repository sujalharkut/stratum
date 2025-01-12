use crate::{
    common_properties::StandardChannel,
    errors::Error,
    utils::{merkle_root_from_path, new_header, new_header_hash, Id, Mutex},
};
use mining_sv2::{
    NewExtendedMiningJob, NewMiningJob, SetNewPrevHash, SubmitSharesError, SubmitSharesStandard,
    Target,
};
use std::{collections::HashMap, convert::TryInto, sync::Arc}; //compact_target_from_u256

fn extended_to_standard_job_for_group_channel<'a>(
    extended: &NewExtendedMiningJob,
    coinbase_script: &[u8],
    channel_id: u32,
    job_id: u32,
) -> NewMiningJob<'a> {
    let merkle_root = merkle_root_from_path(
        extended.coinbase_tx_prefix.inner_as_ref(),
        coinbase_script,
        extended.coinbase_tx_suffix.inner_as_ref(),
        &extended.merkle_path.inner_as_ref(),
    );

    NewMiningJob {
        channel_id,
        job_id,
        future_job: extended.future_job,
        version: extended.version,
        merkle_root: merkle_root.try_into().unwrap(),
    }
}

#[allow(dead_code)]
fn target_from_shares(
    job: &DownstreamJob,
    prev_hash: &[u8],
    nbits: u32,
    share: &SubmitSharesStandard,
) -> Target {
    let header = new_header(
        share.version as i32,
        prev_hash,
        &job.merkle_root,
        share.ntime,
        nbits,
        share.nonce,
    )
    .unwrap();

    new_header_hash(header).try_into().unwrap()
}

// #[derive(Debug)]
// pub struct StandardChannel {
//     target: Target,
//     extranonce: Extranonce,
//     id: u32,
// }

#[derive(Debug)]
struct DownstreamJob {
    merkle_root: Vec<u8>,
    extended_job_id: u32,
}

#[derive(Debug)]
struct ExtendedJobs {
    upstream_target: Vec<u8>,
}

#[derive(Debug)]
pub struct GroupChannelJobDispatcher {
    //channels: Vec<StandardChannel>,
    target: Target,
    prev_hash: Vec<u8>,
    // extedned_job_id -> standard_job_id -> standard_job
    future_jobs: HashMap<u32, HashMap<u32, DownstreamJob>>,
    // standard_job_id -> standard_job
    jobs: HashMap<u32, DownstreamJob>,
    ids: Arc<Mutex<Id>>,
    nbits: u32,
}

pub enum SendSharesResponse {
    //ValidAndMeetUpstreamTarget((SubmitSharesStandard,SubmitSharesSuccess)),
    Valid(SubmitSharesStandard),
    Invalid(SubmitSharesError<'static>),
}

impl GroupChannelJobDispatcher {
    pub fn new(ids: Arc<Mutex<Id>>) -> Self {
        Self {
            target: [0_u8; 32].into(),
            prev_hash: Vec::new(),
            future_jobs: HashMap::new(),
            jobs: HashMap::new(),
            ids,
            nbits: 0,
        }
    }

    pub fn on_new_extended_mining_job(
        &mut self,
        extended: &NewExtendedMiningJob,
        channel: &StandardChannel,
    ) -> NewMiningJob<'static> {
        if extended.future_job {
            self.future_jobs.insert(extended.job_id, HashMap::new());
        };
        let extranonce: Vec<u8> = channel.extranonce.clone().into();
        let new_mining_job_message = extended_to_standard_job_for_group_channel(
            &extended,
            &extranonce,
            channel.channel_id,
            self.ids.safe_lock(|ids| ids.next()).unwrap(),
        );
        let job = DownstreamJob {
            merkle_root: new_mining_job_message.merkle_root.to_vec(),
            extended_job_id: extended.job_id,
        };
        if extended.future_job {
            let future_jobs = self.future_jobs.get_mut(&extended.job_id).unwrap();
            future_jobs.insert(new_mining_job_message.job_id, job);
        } else {
            self.jobs.insert(new_mining_job_message.job_id, job);
        };
        new_mining_job_message
    }

    pub fn on_new_prev_hash(&mut self, message: &SetNewPrevHash) -> Result<(), Error> {
        if self.future_jobs.is_empty() {
            return Err(Error::NoFutureJobs);
        }
        let jobs = match self.future_jobs.get_mut(&message.job_id) {
            Some(j) => j,
            // TODO: What error would exist here? Is there a scenario where a value of
            // message.job_id would cause an error?
            _ => panic!("TODO: What is the appropriate error here?"),
        };
        std::mem::swap(&mut self.jobs, jobs);
        self.prev_hash = message.prev_hash.to_vec();
        self.nbits = message.nbits;
        self.future_jobs.clear();
        Ok(())
    }

    // (response, upstream id)
    pub fn on_submit_shares(&self, shares: SubmitSharesStandard) -> SendSharesResponse {
        let id = shares.job_id;
        if let Some(job) = self.jobs.get(&id) {
            //let target = target_from_shares(
            //    job,
            //    &self.prev_hash,
            //    self.nbits,
            //    &shares,
            //    );
            //match target >= self.target {
            //    true => SendSharesResponse::ValidAndMeetUpstreamTarget(success),
            //    false => SendSharesResponse::Valid(success),
            //}
            let success = SubmitSharesStandard {
                channel_id: shares.channel_id,
                sequence_number: shares.sequence_number,
                job_id: job.extended_job_id,
                nonce: shares.nonce,
                ntime: shares.ntime,
                version: shares.version,
            };
            SendSharesResponse::Valid(success)
        } else {
            let error = SubmitSharesError {
                channel_id: shares.channel_id,
                sequence_number: shares.sequence_number,
                error_code: "".to_string().into_bytes().try_into().unwrap(),
            };
            SendSharesResponse::Invalid(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "serde")]
    use binary_sv2::B032;
    use binary_sv2::{u256_from_int, Seq0255, B064K, U256};
    #[cfg(feature = "serde")]
    use mining_sv2::Extranonce;
    #[cfg(feature = "serde")]
    use serde::Deserialize;

    #[cfg(feature = "serde")]
    use std::convert::TryInto;
    #[cfg(feature = "serde")]
    use std::num::ParseIntError;

    #[cfg(feature = "serde")]
    fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
            .collect()
    }

    #[cfg(feature = "serde")]
    #[derive(Debug, Deserialize)]
    struct TestBlockToml {
        block_hash: String,
        version: u32,
        prev_hash: String,
        time: u32,
        merkle_root: String,
        nbits: u32,
        nonce: u32,
        coinbase_tx_prefix: String,
        coinbase_script: String,
        coinbase_tx_suffix: String,
        path: Vec<String>,
    }

    #[derive(Debug)]
    struct TestBlock<'decoder> {
        block_hash: U256<'decoder>,
        version: u32,
        prev_hash: Vec<u8>,
        time: u32,
        merkle_root: Vec<u8>,
        nbits: u32,
        nonce: u32,
        coinbase_tx_prefix: B064K<'decoder>,
        coinbase_script: Vec<u8>,
        coinbase_tx_suffix: B064K<'decoder>,
        path: Seq0255<'decoder, U256<'decoder>>,
    }

    #[cfg(feature = "serde")]
    fn get_test_block<'decoder>() -> TestBlock<'decoder> {
        let test_file = std::fs::read_to_string("../../../test_data/reg-test-block.toml")
            .expect("Could not read file from string");
        let block: TestBlockToml =
            toml::from_str(&test_file).expect("Could not parse toml file as `TestBlockToml`");

        // Get block hash
        let block_hash_vec =
            decode_hex(&block.block_hash).expect("Could not decode hex string to `Vec<u8>`");
        let mut block_hash_vec: [u8; 32] = block_hash_vec
            .try_into()
            .expect("Slice is incorrect length");
        block_hash_vec.reverse();
        let block_hash: U256 = block_hash_vec
            .try_into()
            .expect("Could not convert `[u8; 32]` to `U256`");

        // Get prev hash
        let mut prev_hash: Vec<u8> =
            decode_hex(&block.prev_hash).expect("Could not convert `String` to `&[u8]`");
        prev_hash.reverse();

        // Get Merkle root
        let mut merkle_root =
            decode_hex(&block.merkle_root).expect("Could not decode hex string to `Vec<u8>`");
        // Swap endianness to LE
        merkle_root.reverse();

        // Get Merkle path
        let mut path_vec = Vec::<U256>::new();
        for p in block.path {
            let p_vec = decode_hex(&p).expect("Could not decode hex string to `Vec<u8>`");
            let p_arr: [u8; 32] = p_vec.try_into().expect("Slice is incorrect length");
            let p_u256: U256 = (p_arr)
                .try_into()
                .expect("Could not convert to `U256` from `[u8; 32]`");
            path_vec.push(p_u256);
        }

        let path = Seq0255::new(path_vec).expect("Could not convert `Vec<U256>` to `Seq0255`");

        // Pass in coinbase as three pieces:
        //   coinbase_tx_prefix + coinbase script + coinbase_tx_suffix
        let coinbase_tx_prefix_vec = decode_hex(&block.coinbase_tx_prefix)
            .expect("Could not decode hex string to `Vec<u8>`");
        let coinbase_tx_prefix: B064K = coinbase_tx_prefix_vec
            .try_into()
            .expect("Could not convert `Vec<u8>` into `B064K`");

        let coinbase_script =
            decode_hex(&block.coinbase_script).expect("Could not decode hex `String` to `Vec<u8>`");

        let coinbase_tx_suffix_vec = decode_hex(&block.coinbase_tx_suffix)
            .expect("Could not decode hex `String` to `Vec<u8>`");
        let coinbase_tx_suffix: B064K = coinbase_tx_suffix_vec
            .try_into()
            .expect("Could not convert `Vec<u8>` to `B064K`");

        TestBlock {
            block_hash,
            version: block.version,
            prev_hash,
            time: block.time,
            merkle_root,
            nbits: block.nbits,
            nonce: block.nonce,
            coinbase_tx_prefix,
            coinbase_script,
            coinbase_tx_suffix,
            path,
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn success_extended_to_standard_job_for_group_channel() {
        let channel_id = 0;
        let job_id = 0;
        let future_job = true; // RR TODO: test with false
        let block = get_test_block();
        let merkle_root: B032 = block.merkle_root.try_into().expect("Invalid `B032`");

        let expect = NewMiningJob {
            channel_id,
            job_id,
            future_job,
            version: 2,
            merkle_root,
        };

        let extended = NewExtendedMiningJob {
            channel_id,
            job_id,
            future_job: true, // RR TODO: test w false
            version: 2,
            version_rolling_allowed: true, // RR TODO: test w false
            merkle_path: block.path,
            coinbase_tx_prefix: block.coinbase_tx_prefix,
            coinbase_tx_suffix: block.coinbase_tx_suffix,
        };

        let actual = extended_to_standard_job_for_group_channel(
            &extended,
            &block.coinbase_script,
            channel_id,
            job_id,
        );

        assert_eq!(actual, expect);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn error_if_unexpected_len_on_new_header() -> Result<(), ()> {
        // Test that it error on bad prev_hash
        let block = get_test_block();
        let bad_prev_hash = vec![0u8; 31];

        let err = new_header(
            block.version as i32,
            &bad_prev_hash,
            &block.merkle_root,
            block.time,
            block.nbits,
            block.nonce,
        )
        .unwrap_err();

        let expect = String::from("Expected length of 32, but received length of 31");
        assert_eq!(err.to_string(), expect);

        // Test that it error on bad merkle_root
        let block = get_test_block();
        let bad_merkle_root = vec![0u8; 31];

        let err = new_header(
            block.version as i32,
            &block.prev_hash,
            &bad_merkle_root,
            block.time,
            block.nbits,
            block.nonce,
        )
        .unwrap_err();

        let expect = String::from("Expected length of 32, but received length of 31");
        assert_eq!(err.to_string(), expect);

        Ok(())
    }

    #[test]
    #[cfg(feature = "serde")]
    fn gets_target_from_shares() {
        let block = get_test_block();
        let expect: Target = block.block_hash.try_into().unwrap();

        let job = DownstreamJob {
            merkle_root: block.merkle_root,
            extended_job_id: 0,
        };
        let share = SubmitSharesStandard {
            channel_id: 0,
            sequence_number: 0xfffffffe, // dummy var
            job_id: 0,
            nonce: block.nonce,
            ntime: block.time,
            version: block.version,
        };

        let actual = target_from_shares(&job, &block.prev_hash, block.nbits, &share);

        assert_eq!(actual, expect);
    }

    #[test]
    fn builds_group_channel_job_dispatcher() {
        let expect = GroupChannelJobDispatcher {
            target: [0_u8; 32].into(),
            prev_hash: Vec::new(),
            future_jobs: HashMap::new(),
            jobs: HashMap::new(),
            ids: Arc::new(Mutex::new(Id::new())),
            nbits: 0,
        };

        let ids = Arc::new(Mutex::new(Id::new()));
        let actual = GroupChannelJobDispatcher::new(ids);

        assert_eq!(expect.target, actual.target);
        assert_eq!(expect.prev_hash, actual.prev_hash);
        assert_eq!(expect.nbits, actual.nbits);
        assert!(actual.future_jobs.is_empty());
        assert!(actual.jobs.is_empty());
        // TODO: check actual.ids, but idk how to properly test arc
        // assert_eq!(expect.ids, actual.ids);
    }

    #[ignore]
    #[test]
    #[cfg(feature = "serde")]
    fn updates_group_channel_job_dispatcher_on_new_extended_mining_job() {
        let channel_id = 0;
        let job_id = 0;
        let future_job = false; // RR TODO: test with true

        let block = get_test_block();
        let merkle_root: B032 = block.merkle_root.try_into().expect("Invalid `B032`");

        let expect = NewMiningJob {
            channel_id,
            job_id,
            future_job: true,
            version: 2,
            merkle_root,
        };

        let ids = Arc::new(Mutex::new(Id::new()));
        let mut dispatcher = GroupChannelJobDispatcher::new(ids);
        let extended = NewExtendedMiningJob {
            channel_id,
            job_id,
            future_job,
            version: 2,
            version_rolling_allowed: true,
            merkle_path: block.path,
            coinbase_tx_prefix: block.coinbase_tx_prefix,
            coinbase_tx_suffix: block.coinbase_tx_suffix,
        };

        let extranonce = Extranonce::new();
        let target: Target = ([
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0b_0001_0000,
            0_u8,
        ])
        .try_into()
        .expect("Could not convert `[u8; 32]` to `Target`");
        let channel = StandardChannel {
            channel_id,
            group_id: 1,
            target,
            extranonce,
        };

        println!("DISPATCHER 1: {:?}", &dispatcher);
        let actual = dispatcher.on_new_extended_mining_job(&extended, &channel);
        println!("DISPATCHER 2: {:?}", &dispatcher);

        assert_eq!(actual, expect);
    }

    #[ignore]
    #[test]
    fn updates_group_channel_job_dispatcher_on_new_prev_hash() -> Result<(), Error> {
        let message = SetNewPrevHash {
            channel_id: 0,
            job_id: 0,
            prev_hash: u256_from_int(45_u32),
            min_ntime: 0,
            nbits: 0,
        };
        let ids = Arc::new(Mutex::new(Id::new()));
        let mut dispatcher = GroupChannelJobDispatcher::new(ids);

        // TODO: fails on self.future_jobs unwrap in the first line of the on_new_prev_hash fn
        let _actual = dispatcher.on_new_prev_hash(&message)?;
        // let actual_prev_hash: U256<'static> = u256_from_int(tt);
        let expect_prev_hash: Vec<u8> = dispatcher.prev_hash.to_vec();
        // assert_eq!(expect_prev_hash, dispatcher.prev_hash);
        //
        assert_eq!(expect_prev_hash, dispatcher.prev_hash);

        Ok(())
    }

    #[test]
    fn fails_to_update_group_channel_job_dispatcher_on_new_prev_hash_if_no_future_jobs() {
        let message = SetNewPrevHash {
            channel_id: 0,
            job_id: 0,
            prev_hash: u256_from_int(45_u32),
            min_ntime: 0,
            nbits: 0,
        };
        let ids = Arc::new(Mutex::new(Id::new()));
        let mut dispatcher = GroupChannelJobDispatcher::new(ids);

        let err = dispatcher.on_new_prev_hash(&message).unwrap_err();
        assert_eq!(
            err.to_string(),
            "GroupChannelJobDispatcher does not have any future jobs"
        );
    }
}
