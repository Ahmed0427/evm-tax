//! Runs tests/contract_testcases/riscv/incrementer.sol from Solang in SP1.
//!
//!   cargo run --release             execute four transactions (no proofs)
//!   cargo run --release -- --prove  prove inc(5), verify it, then tamper with it

use sp1_sdk::blocking::{CpuProver, ProveRequest, Prover, ProverClient};
use sp1_sdk::{include_elf, Elf, HashableKey, ProvingKey, SP1PublicValues, SP1Stdin};
use std::time::Instant;
use tiny_keccak::{Hasher, Keccak};

const ELF: Elf = include_elf!("solang-guest");

type Word = [u8; 32];
type Storage = Vec<(Word, Word)>;

/// What the guest commits, in the order it commits it.
#[derive(Debug, PartialEq)]
struct Public {
    deploy: bool,
    calldata: Vec<u8>,
    storage_before: Storage,
    reverted: bool,
    output: Vec<u8>,
    storage_after: Storage,
}

impl Public {
    fn read(public: &mut SP1PublicValues) -> Self {
        Public {
            deploy: public.read(),
            calldata: public.read(),
            storage_before: public.read(),
            reverted: public.read(),
            output: public.read(),
            storage_after: public.read(),
        }
    }

    fn write(&self) -> SP1PublicValues {
        let mut public = SP1PublicValues::new();
        public.write(&self.deploy);
        public.write(&self.calldata);
        public.write(&self.storage_before);
        public.write(&self.reverted);
        public.write(&self.output);
        public.write(&self.storage_after);
        public
    }

    fn print(&self) {
        println!("  deploy:         {}", self.deploy);
        println!("  calldata:       0x{}", hex(&self.calldata));
        println!("  storage before: {}", storage(&self.storage_before));
        println!("  reverted:       {}", self.reverted);
        println!("  output:         0x{}", hex(&self.output));
        println!("  storage after:  {}", storage(&self.storage_after));
    }
}

fn stdin(deploy: bool, calldata: &[u8], storage: &Storage) -> SP1Stdin {
    let mut stdin = SP1Stdin::new();
    stdin.write(&deploy);
    stdin.write(&calldata.to_vec());
    stdin.write(storage);
    stdin
}

fn execute(prover: &CpuProver, label: &str, deploy: bool, calldata: Vec<u8>, storage: &Storage) -> Public {
    println!("\n== {label}");
    // The guest's println! output, which logs each __sys_* call.
    let (guest_stdout, log) = tokio::sync::watch::channel(String::new());
    let (mut values, report) = prover
        .execute(ELF, stdin(deploy, &calldata, storage))
        .stdout(guest_stdout)
        .run()
        .expect("guest failed");
    print!("{}", *log.borrow());
    println!("  {} cycles", report.total_instruction_count());

    let public = Public::read(&mut values);
    assert_eq!(public.calldata, calldata);
    assert_eq!(&public.storage_before, storage);
    public
}

fn word(n: u64) -> Word {
    let mut word = [0; 32];
    word[24..].copy_from_slice(&n.to_be_bytes());
    word
}

fn call(signature: &str, args: &[Word]) -> Vec<u8> {
    let mut hash = [0; 32];
    let mut keccak = Keccak::v256();
    keccak.update(signature.as_bytes());
    keccak.finalize(&mut hash);
    let mut calldata = hash[..4].to_vec();
    args.iter().for_each(|arg| calldata.extend_from_slice(arg));
    calldata
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn storage(storage: &Storage) -> String {
    let short = |w: &Word| u128::from_be_bytes(w[16..].try_into().unwrap());
    let slots: Vec<_> = storage
        .iter()
        .map(|(k, v)| format!("slot{} = {}", short(k), short(v)))
        .collect();
    format!("{{{}}}", slots.join(", "))
}

fn run_transactions(prover: &CpuProver) {
    let deployed = execute(prover, "deploy(5)", true, word(5).to_vec(), &vec![]);
    deployed.print();
    assert_eq!(deployed.storage_after, vec![(word(0), word(5))]);

    let inc = execute(prover, "inc(5)", false, call("inc(uint32)", &[word(5)]), &deployed.storage_after);
    inc.print();
    assert_eq!(inc.storage_after, vec![(word(0), word(10))]);

    let get = execute(prover, "get()", false, call("get()", &[]), &inc.storage_after);
    get.print();
    assert_eq!(get.output, word(10));

    let nope = execute(prover, "nope()", false, call("nope()", &[]), &inc.storage_after);
    nope.print();
    assert!(nope.reverted);
    assert_eq!(nope.storage_after, inc.storage_after);

    println!("\nall good");
}

fn prove_inc(prover: &CpuProver) {
    let storage = vec![(word(0), word(5))];
    let calldata = call("inc(uint32)", &[word(5)]);

    // The verifying key identifies the program: the guest with this contract
    // linked in. A proof is only valid for that key.
    let start = Instant::now();
    let pk = prover.setup(ELF).expect("setup");
    println!("setup: {:?}, vkey {}", start.elapsed(), pk.verifying_key().bytes32());

    println!("\n== proving inc(5) on {}", self::storage(&storage));
    let start = Instant::now();
    let mut proof = prover
        .prove(&pk, stdin(false, &calldata, &storage))
        .core()
        .run()
        .expect("proving failed");
    println!("proved in {:?}, {} bytes", start.elapsed(), proof.bytes().len());

    let public = Public::read(&mut proof.public_values.clone());
    println!("the proof commits to:");
    public.print();

    prover
        .verify(&proof, pk.verifying_key(), None)
        .expect("a valid proof verifies");
    println!("\nverify(proof): ok");

    // Claim slot0 = 99 instead of 10. The public values no longer hash to
    // the digest inside the proof, so verification must fail.
    let forged = Public {
        storage_after: vec![(word(0), word(99))],
        ..public
    };
    proof.public_values = forged.write();

    let result = prover.verify(&proof, pk.verifying_key(), None);
    println!(
        "verify(proof claiming slot0 = 99): {}",
        if result.is_err() { "rejected" } else { "ACCEPTED" }
    );
    assert!(result.is_err(), "a tampered proof must not verify");
}

fn main() {
    let prover = ProverClient::builder().cpu().build();

    if std::env::args().any(|arg| arg == "--prove") {
        prove_inc(&prover);
    } else {
        run_transactions(&prover);
    }
}
