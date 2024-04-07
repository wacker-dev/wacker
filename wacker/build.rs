use anyhow::Result;

fn main() -> Result<()> {
    tonic_build::compile_protos("proto/wacker.proto")?;
    Ok(())
}
