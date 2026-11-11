//! Ciphertext Blowup Measurement — tfhe-rs v1.6.2
//!
//! يقيس الحجم الحقيقي للبيانات المشفرة بعد serialization
//! لتحديد متطلبات شبكة QUIC في المرحلة القادمة

use tfhe::prelude::*;
use tfhe::{generate_keys, CompressedServerKey, ConfigBuilder};

fn measure_u8(val: u8, ck: &tfhe::ClientKey) -> Vec<u8> {
    let enc = tfhe::FheUint8::encrypt(val, ck);
    let ser = bincode::serialize(&enc).unwrap();
    let blowup = ser.len() as f64 / std::mem::size_of_val(&val) as f64;
    println!("  │ {:<12} │ {:>8} b │ {:>10} b │ {:>8.2} KB │ {:>8.1}× │",
        "FheUint8", std::mem::size_of_val(&val), ser.len(), ser.len() as f64 / 1024.0, blowup);
    ser
}

fn measure_u16(val: u16, ck: &tfhe::ClientKey) -> Vec<u8> {
    let enc = tfhe::FheUint16::encrypt(val, ck);
    let ser = bincode::serialize(&enc).unwrap();
    let blowup = ser.len() as f64 / std::mem::size_of_val(&val) as f64;
    println!("  │ {:<12} │ {:>8} b │ {:>10} b │ {:>8.2} KB │ {:>8.1}× │",
        "FheUint16", std::mem::size_of_val(&val), ser.len(), ser.len() as f64 / 1024.0, blowup);
    ser
}

fn measure_u32(val: u32, ck: &tfhe::ClientKey) -> Vec<u8> {
    let enc = tfhe::FheUint32::encrypt(val, ck);
    let ser = bincode::serialize(&enc).unwrap();
    let blowup = ser.len() as f64 / std::mem::size_of_val(&val) as f64;
    println!("  │ {:<12} │ {:>8} b │ {:>10} b │ {:>8.2} KB │ {:>8.1}× │",
        "FheUint32", std::mem::size_of_val(&val), ser.len(), ser.len() as f64 / 1024.0, blowup);
    ser
}

fn measure_u64(val: u64, ck: &tfhe::ClientKey) -> Vec<u8> {
    let enc = tfhe::FheUint64::encrypt(val, ck);
    let ser = bincode::serialize(&enc).unwrap();
    let blowup = ser.len() as f64 / std::mem::size_of_val(&val) as f64;
    println!("  │ {:<12} │ {:>8} b │ {:>10} b │ {:>8.2} KB │ {:>8.1}× │",
        "FheUint64", std::mem::size_of_val(&val), ser.len(), ser.len() as f64 / 1024.0, blowup);
    ser
}

fn main() {
    println!("══════════════════════════════════════════════════════════════");
    println!("  TFHE-rs v1.6.2 — Ciphertext Blowup Measurement");
    println!("  Config: default (128-bit security, integer feature)");
    println!("══════════════════════════════════════════════════════════════");
    println!();

    let config = ConfigBuilder::default().build();
    let (client_key, server_keys) = generate_keys(config);
    tfhe::set_server_key(server_keys.clone());

    println!("  ┌──────────────┬──────────┬────────────┬───────────┬──────────┐");
    println!("  │ Type         │ Plain    │ Ciphertext │ Size      │ Blowup   │");
    println!("  ├──────────────┼──────────┼────────────┼───────────┼──────────┤");

    let _ = measure_u8(42u8, &client_key);
    let _ = measure_u16(42u16, &client_key);
    let _ = measure_u32(42u32, &client_key);
    let _ = measure_u64(42u64, &client_key);

    // Compound result: sum
    let a = tfhe::FheUint32::encrypt(100u32, &client_key);
    let b = tfhe::FheUint32::encrypt(200u32, &client_key);
    let sum = &a + &b;
    let sum_bytes = bincode::serialize(&sum).unwrap();
    println!(
        "  │ {:<12} │ {:>8} b │ {:>10} b │ {:>8.2} KB │ {:>8.1}× │",
        "FheUint32+", 8, sum_bytes.len(), sum_bytes.len() as f64 / 1024.0,
        sum_bytes.len() as f64 / 8.0,
    );

    let prod = &a * &b;
    let prod_bytes = bincode::serialize(&prod).unwrap();
    println!(
        "  │ {:<12} │ {:>8} b │ {:>10} b │ {:>8.2} KB │ {:>8.1}× │",
        "FheUint32*", 8, prod_bytes.len(), prod_bytes.len() as f64 / 1024.0,
        prod_bytes.len() as f64 / 8.0,
    );

    // Compressed ServerKey (for network transfer)
    let compressed_sk = CompressedServerKey::new(&client_key);
    let csk_bytes = bincode::serialize(&compressed_sk).unwrap();
    println!(
        "  │ {:<12} │ {:>8}   │ {:>10} b │ {:>8.2} KB │ {:>8}   │",
        "CompSK", "-", csk_bytes.len(), csk_bytes.len() as f64 / 1024.0, "-",
    );

    // Uncompressed ServerKey (in-memory on relay)
    let sk_bytes = bincode::serialize(&server_keys).unwrap();
    println!(
        "  │ {:<12} │ {:>8}   │ {:>10} b │ {:>8.2} KB │ {:>8}   │",
        "ServerKey", "-", sk_bytes.len(), sk_bytes.len() as f64 / 1024.0, "-",
    );

    let ck_bytes = bincode::serialize(&client_key).unwrap();
    println!(
        "  │ {:<12} │ {:>8}   │ {:>10} b │ {:>8.2} KB │ {:>8}   │",
        "ClientKey", "-", ck_bytes.len(), ck_bytes.len() as f64 / 1024.0, "-",
    );

    println!("  └──────────────┴──────────┴────────────┴───────────┴──────────┘");
    println!();

    let ct = tfhe::FheUint32::encrypt(0u32, &client_key);
    let ct_size = bincode::serialize(&ct).unwrap().len();
    println!("══════════════════════════════════════════════════════════════");
    println!("  📌 Implications for BDCP network layer:");
    println!();
    println!("  • Each ciphertext: ~{:.0} KB for 32-bit integers", ct_size as f64 / 1024.0);
    println!("  • Blowup factor: > {}×", ct_size / 4);
    println!("  • CompressedServerKey (CSPRNG seed): ~{:.0} KB", csk_bytes.len() as f64 / 1024.0);
    println!("  • ServerKey (decompressed, in-memory): ~{:.0} KB", sk_bytes.len() as f64 / 1024.0);
    println!("  • Compression ratio: {:.1}×", sk_bytes.len() as f64 / csk_bytes.len() as f64);
    println!("  • A single 10-ciphertext payload: ~{:.0} KB",
             ct_size as f64 * 10.0 / 1024.0);
    println!("══════════════════════════════════════════════════════════════");
}
