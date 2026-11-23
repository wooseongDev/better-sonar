use std::{env, fs, path::Path};

use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use minisign::{PublicKeyBox, SecretKeyBox};
use minisign_verify::{PublicKey, Signature};
use semver::Version;

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let public_key = args
        .next()
        .context("usage: better-sonar-sign <public-key-file> <version> <installer>...")?;
    let version: Version = args
        .next()
        .context("missing version")?
        .into_string()
        .map_err(|_| anyhow::anyhow!("invalid version"))?
        .parse()?;
    let paths: Vec<_> = args.collect();
    ensure!(!paths.is_empty(), "missing installer path");

    let secret = env::var("BETTER_SONAR_SIGNING_KEY").context("BETTER_SONAR_SIGNING_KEY is required")?;
    let password = env::var("BETTER_SONAR_SIGNING_KEY_PASSWORD").unwrap_or_default();
    let secret = decode_secret(&secret)?;
    let secret = SecretKeyBox::from_string(&secret)?.into_secret_key(Some(password))?;
    let public = fs::read_to_string(public_key)?;
    let public = String::from_utf8(STANDARD.decode(public.trim())?)?;
    let signer_public = PublicKeyBox::from_string(&public)?.into_public_key()?;
    let verifier_public = PublicKey::decode(&public)?;

    for path in &paths {
        let path = Path::new(path);
        let mut file = fs::File::open(path)?;
        let comment = format!("version:{version}");
        let signature = minisign::sign(Some(&signer_public), &secret, &mut file, Some(&comment), None)?;
        let signature = signature.to_string();
        verifier_public.verify(&fs::read(path)?, &Signature::decode(&signature)?, false)?;
        let mut sidecar = path.as_os_str().to_owned();
        sidecar.push(".sig");
        fs::write(sidecar, STANDARD.encode(signature))?;
    }
    Ok(())
}

fn decode_secret(secret: &str) -> Result<String> {
    let secret = secret.trim();
    if secret.starts_with("untrusted comment: ") {
        return Ok(secret.to_owned());
    }
    Ok(String::from_utf8(
        STANDARD.decode(secret).context("invalid signing key encoding")?,
    )?)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use minisign::KeyPair;

    use super::*;

    #[test]
    fn signature_binds_installer_bytes_and_version() {
        let pair = KeyPair::generate_unencrypted_keypair().unwrap();
        let data = b"installer";
        let signature =
            minisign::sign(Some(&pair.pk), &pair.sk, Cursor::new(data), Some("version:0.0.7"), None).unwrap();
        let public = PublicKey::decode(&pair.pk.to_box().unwrap().to_string()).unwrap();
        let signature = Signature::decode(&signature.to_string()).unwrap();
        assert_eq!(signature.trusted_comment(), "version:0.0.7");
        assert!(public.verify(data, &signature, false).is_ok());
        assert!(public.verify(b"tampered installer", &signature, false).is_err());
    }
}
