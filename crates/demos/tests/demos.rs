//! Each demo's image is what its source assembles to. After a change meant
//! to change one, `JUKU_ASM_BLESS=1 cargo test -p juku-demos` writes the
//! images anew.

use std::path::Path;

use juku_demos::DEMOS;

#[test]
fn images_are_the_sources() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos");
    for demo in DEMOS {
        let image = dir.join(format!("{}.bin", demo.name));
        let program = juku_asm::assemble(&dir.join(format!("{}.asm", demo.name))).unwrap();
        assert_eq!(program.load, 0x100, "{}", demo.name);
        if std::env::var_os("JUKU_ASM_BLESS").is_some() {
            std::fs::write(&image, &program.image).unwrap();
        } else {
            assert!(program.image == demo.image, "{}.bin is not what {}.asm assembles to", demo.name, demo.name);
        }
    }
}
