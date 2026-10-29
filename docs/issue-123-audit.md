# #123 turned-body audit

Against main `42649a258f616301f7faefee53af68d3ada3f049`, with the full read-only corpus pin `6d544e060cd0eeb4442d2196e5df757b5a15ea55` and every available captured/other STEP fixture. The new original test disc is included. Input digests, both raw and published body inventories, every suppression and refusal witness are in [issue-123-audit.json](issue-123-audit.json).

2,964 inputs: 2,953 readable; 11 unchanged baseline reader refusals. Raw and published output each gain **187 turned bodies** over 162 qualifying inputs, with **27 OD-boss suppressions**. There are **zero other existing-record or defining-face changes**. Existing records were compared without numeric tolerances. Body face-accounting references gain the new co-owner.

Of 111 inputs with existing `turned_steps`, **92 qualify** and **19 do not**. All 19 have an explicit incompatible-axis or nontransverse-plane witness; none is refused merely because it has one OD. The pinned corpus contributes seven such inputs: two qualify and five have those incompatible supports. The flanged spool and its captured variants include offset axes; string-post variants include a perpendicular cross cylinder. NIST CTC-05 has transverse-to-turning side planes and machining details. Certified NURBS planar shoulders make both dedicated captured shaft fixtures qualify (including both solids in the assembly).

## Unattended decisions and gate

- Model precision is `max(file uncertainty, COORD_FLOOR)`; no frame or minimum-step gate. Different axes on one solid refuse; separate solids qualify independently.
- A complete positive-weight NURBS control net inside the plane slab is sufficient plane proof. A sampled fit alone is not.
- A complete sphere uses its native polar axis as a valid representative of its unconstrained axes, so the representative follows rigid motion.
- OD-boss suppression uses raw cylinder geometry: diameter equals OD within model precision, span at least 90% of length, and each end gap at most 5% of length plus that precision. Smaller/shorter end bosses remain.
- The clean audit gate above authorises re-recording under the unattended instruction; no maintainer answer was requested. Baseline malformed fixtures retain exactly their existing reader errors.
- The unlicensed 608ZZ STEP remains excluded; only measured audit facts are recorded. The disc fixture was generated independently from the supplied dimensions using the repository script.

## 608ZZ

One Z-axis body, OD 22, length 7, bore diameter 8 over [-3.5, 3.5]. All 14 faces co-own the body. The meridian includes the two 0.25 x 45-degree OD chamfers and all recess surfaces. Boss face 3 (diameter 22, raw span 6.5, body length 7) is suppressed. The two chamfers and through hole are exactly unchanged. No end-recess feature recogniser was added.

## Every new turned body (published dimensions)

Full-precision values and the corresponding raw axis representatives are retained in the JSON. Rounded display below is for reading only.

| Part | Body | OD | Length | Axis |
|---|---:|---:|---:|---|
| `corpus/gramel/GRM-03_thumbwheel_drive_screw.step` | 0 | 10 | 23.5 | x |
| `corpus/gramel/GRM-05_depth_lock_bolt.step` | 0 | 10 | 32 | z |
| `repro/bearing-608zz.step` | 0 | 22 | 7 | z |
| `fixture/annular_face_groove.step` | 0 | 24 | 10 | z |
| `fixture/ap242/assembly/assembly.step` | 0 | 10 | 32 | z |
| `fixture/ap242/assembly/assembly.step` | 1 | 10 | 32 | [0.364833195, -0.0745427634, 0.928084111] |
| `fixture/ap242/assembly/editor_assembly.step` | 0 | 10 | 32 | z |
| `fixture/ap242/assembly/editor_assembly.step` | 1 | 10 | 32 | z |
| `fixture/ap242/assembly/nested.step` | 0 | 10 | 32 | z |
| `fixture/ap242/assembly/nested.step` | 1 | 10 | 32 | [0.364833195, -0.0745427634, 0.928084111] |
| `fixture/ap242/derived/GRM-03_thumbwheel_drive_screw_AP242_PMI.step.gz` | 0 | 10 | 28.7 | x |
| `fixture/ap242/specify/assembly_plate_pin.step.gz` | 0 | 10 | 32 | z |
| `fixture/ap242/specify/assembly_plate_pin.step.gz` | 1 | 10 | 32 | [0.364833195, -0.0745427634, 0.928084111] |
| `fixture/ap242/specify/bolt_thread_knurl.step.gz` | 0 | 10 | 32 | z |
| `fixture/ap242/specify/thumbwheel_thread_knurl.step.gz` | 0 | 10 | 23.5 | x |
| `fixture/ap242/write/assembly_plate_pin_intent.step.gz` | 0 | 10 | 32 | z |
| `fixture/ap242/write/assembly_plate_pin_intent.step.gz` | 1 | 10 | 32 | [0.364833195, -0.0745427634, 0.928084111] |
| `fixture/ap242/write/editor_assembly_pin_alone.step.gz` | 0 | 10 | 32 | z |
| `fixture/ap242/write/editor_assembly_pin_alone.step.gz` | 1 | 10 | 32 | z |
| `fixture/ap242/write/thumbwheel_notes_add.step.gz` | 0 | 10 | 23.5 | x |
| `fixture/ap242/write/thumbwheel_thread_knurl_intent.step.gz` | 0 | 10 | 23.5 | x |
| `fixture/captured/00f9e4395566750f.step.gz` | 0 | 30 | 40 | z |
| `fixture/captured/0a3141da4c6d054d.step.gz` | 0 | 30 | 70 | z |
| `fixture/captured/0bc17895a014bf83.step.gz` | 0 | 20 | 40 | z |
| `fixture/captured/0bf9d1693184d785.step.gz` | 0 | 40 | 50 | z |
| `fixture/captured/1064acdd3a453258.step.gz` | 0 | 150 | 255 | z |
| `fixture/captured/12dc71de3566ba7f.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/1590cfd2fcde58fd.step.gz` | 0 | 30 | 70 | z |
| `fixture/captured/1590cfd2fcde58fd.step.gz` | 1 | 30 | 70 | z |
| `fixture/captured/19b8517f08e79524.step.gz` | 0 | 30 | 60 | y |
| `fixture/captured/1bdbb95626555d52.step.gz` | 0 | 40 | 80 | z |
| `fixture/captured/1e52d6a18a3a45c5.step.gz` | 0 | 30 | 60 | z |
| `fixture/captured/235d9e4d3a5e8dbe.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/247a52f98035a214.step.gz` | 0 | 150 | 550 | x |
| `fixture/captured/2727a1b2c7ee0f3e.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/2733401823bf7439.step.gz` | 0 | 24 | 8 | z |
| `fixture/captured/2733401823bf7439.step.gz` | 1 | 24 | 8 | z |
| `fixture/captured/29588a55e9ed48bc.step.gz` | 0 | 30 | 70 | x |
| `fixture/captured/2bc1f6aedaf3beef.step.gz` | 0 | 30 | 70 | x |
| `fixture/captured/2bc1f6aedaf3beef.step.gz` | 1 | 30 | 70 | y |
| `fixture/captured/2c8ff38cf754d67e.step.gz` | 0 | 10 | 10 | z |
| `fixture/captured/2c8ff38cf754d67e.step.gz` | 1 | 10 | 10 | z |
| `fixture/captured/2e68dc4f186869d0.step.gz` | 0 | 30 | 60 | z |
| `fixture/captured/33da79af1a2c7e2f.step.gz` | 0 | 30 | 70 | x |
| `fixture/captured/33da79af1a2c7e2f.step.gz` | 1 | 30 | 70 | x |
| `fixture/captured/37e69d8b9cffab0b.step.gz` | 0 | 30 | 110 | x |
| `fixture/captured/37f06bc6c2b9274a.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/3851ade112550bd3.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/39337755971cc7e4.step.gz` | 0 | 20 | 40 | x |
| `fixture/captured/394986f0159b18c2.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/3aa8de56630d5a8f.step.gz` | 0 | 30 | 20 | z |
| `fixture/captured/3aa8de56630d5a8f.step.gz` | 1 | 24 | 5 | z |
| `fixture/captured/3aa8de56630d5a8f.step.gz` | 2 | 30 | 20 | z |
| `fixture/captured/3d79052365d2d959.step.gz` | 0 | 40 | 80 | z |
| `fixture/captured/3d79052365d2d959.step.gz` | 1 | 10 | 80 | z |
| `fixture/captured/40574cca69345a01.step.gz` | 0 | 20 | 2 | [-0, 0.707106781, -0.707106781] |
| `fixture/captured/4a15611bcaf9623a.step.gz` | 0 | 15 | 42.5 | z |
| `fixture/captured/4a8a743698bc98d6.step.gz` | 0 | 30 | 40 | z |
| `fixture/captured/4b0e326a76a59f56.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/536101a86c5b99db.step.gz` | 0 | 40 | 80 | z |
| `fixture/captured/539c59365c1b2b11.step.gz` | 0 | 40 | 80 | z |
| `fixture/captured/539c59365c1b2b11.step.gz` | 1 | 10 | 80 | z |
| `fixture/captured/5415d7192b9ace39.step.gz` | 0 | 40 | 80 | z |
| `fixture/captured/56ba1587a0ea7977.step.gz` | 0 | 40 | 80 | y |
| `fixture/captured/57d77ea643b500bd.step.gz` | 0 | 40 | 80 | x |
| `fixture/captured/5825e2f70daa9e6b.step.gz` | 0 | 22 | 22 | z |
| `fixture/captured/58bbd31f6ae096c4.step.gz` | 0 | 30 | 70 | x |
| `fixture/captured/5a267669f1e09941.step.gz` | 0 | 30 | 51 | z |
| `fixture/captured/5a7cbebb94a3610a.step.gz` | 0 | 20 | 30 | x |
| `fixture/captured/5c757e3242e218b6.step.gz` | 0 | 30 | 70 | z |
| `fixture/captured/5d85fadf9031e41d.step.gz` | 0 | 1.5 | 5.5 | x |
| `fixture/captured/5e0b3537de7bd4f6.step.gz` | 0 | 30 | 60 | x |
| `fixture/captured/5fd5f92d41a072aa.step.gz` | 0 | 40 | 80 | z |
| `fixture/captured/5fd5f92d41a072aa.step.gz` | 1 | 40 | 80 | z |
| `fixture/captured/61c339cd91aad581.step.gz` | 0 | 1.5 | 3.5 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/61dfa6676b478242.step.gz` | 0 | 1.5 | 3.5 | z |
| `fixture/captured/63c86de4afa32ec2.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/6fb28ecc946d5d9e.step.gz` | 0 | 40 | 50 | z |
| `fixture/captured/71ddf8cdbddc0bf6.step.gz` | 0 | 30 | 40 | z |
| `fixture/captured/736989458c598fbd.step.gz` | 0 | 30 | 35 | z |
| `fixture/captured/75c9a38a5f509c16.step.gz` | 0 | 30 | 70 | z |
| `fixture/captured/7a0c25ea430dc5d3.step.gz` | 0 | 30 | 70 | z |
| `fixture/captured/7cfea63cff81bbdb.step.gz` | 0 | 30 | 70 | x |
| `fixture/captured/7cfea63cff81bbdb.step.gz` | 1 | 30 | 70 | x |
| `fixture/captured/7df43565af5f6c51.step.gz` | 0 | 30 | 70 | z |
| `fixture/captured/80959c701f2dbca2.step.gz` | 0 | 3000 | 7000 | z |
| `fixture/captured/80be23086aa30487.step.gz` | 0 | 30 | 60 | x |
| `fixture/captured/817857f3c72ff64a.step.gz` | 0 | 40 | 39.1918359 | z |
| `fixture/captured/81d1dd929f92190e.step.gz` | 0 | 1.5 | 2.55 | z |
| `fixture/captured/82f881772a43717f.step.gz` | 0 | 40 | 80 | x |
| `fixture/captured/838d0be6a2deec8b.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/83bd0bbbd4ce54f1.step.gz` | 0 | 24 | 20 | z |
| `fixture/captured/8adc2bd9a63bf156.step.gz` | 0 | 20 | 40 | x |
| `fixture/captured/8adc57b328863b43.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/8cd5c2e86c7a8c9c.step.gz` | 0 | 24 | 4 | z |
| `fixture/captured/8dba2c172dfc2a1c.step.gz` | 0 | 40 | 80 | z |
| `fixture/captured/8dba2c172dfc2a1c.step.gz` | 1 | 40 | 80 | z |
| `fixture/captured/8de4e531cc062245.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/8e7d28fcf216a734.step.gz` | 0 | 30 | 45 | z |
| `fixture/captured/8ef258defb12d32b.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/91a001e2edbe17aa.step.gz` | 0 | 30 | 70 | x |
| `fixture/captured/93bc8ca0db4db566.step.gz` | 0 | 300 | 42.5 | z |
| `fixture/captured/9506729f4e46c87d.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/9808e56042d8be45.step.gz` | 0 | 20 | 34 | z |
| `fixture/captured/98be6b713d241f01.step.gz` | 0 | 30 | 70 | z |
| `fixture/captured/9af1f99c3f98ddff.step.gz` | 0 | 30 | 70 | z |
| `fixture/captured/9b41e43165338e4b.step.gz` | 0 | 50 | 6 | z |
| `fixture/captured/a09658aa8d8c9f87.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/a28b77dcec31243c.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/af713cf458681f8c.step.gz` | 0 | 30 | 70 | x |
| `fixture/captured/b2768db806dfc5f5.step.gz` | 0 | 40 | 50 | z |
| `fixture/captured/b8f731787296773a.step.gz` | 0 | 30 | 70 | x |
| `fixture/captured/b9503ffca5dae973.step.gz` | 0 | 100 | 42.5 | z |
| `fixture/captured/bdfcfcbb8f4605f0.step.gz` | 0 | 3000 | 7000 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/c206ed1b01003498.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/c2eaf70c9c04bbb6.step.gz` | 0 | 24 | 20 | [-0, 0.707106781, -0.707106781] |
| `fixture/captured/c5946271f97e763b.step.gz` | 0 | 30 | 70 | x |
| `fixture/captured/c5946271f97e763b.step.gz` | 1 | 24 | 56 | x |
| `fixture/captured/c71ff37912b52ee7.step.gz` | 0 | 30 | 70 | x |
| `fixture/captured/c7c8828d0fdecf5f.step.gz` | 0 | 40 | 80 | z |
| `fixture/captured/c946861a441710cf.step.gz` | 0 | 30 | 32 | z |
| `fixture/captured/c9b77f12d7b16325.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/ca690ea4d4850863.step.gz` | 0 | 40 | 80 | z |
| `fixture/captured/cab1441eeee094f5.step.gz` | 0 | 20 | 40 | x |
| `fixture/captured/cc83b5ca1180b368.step.gz` | 0 | 30 | 51 | z |
| `fixture/captured/cde47973f94e2726.step.gz` | 0 | 150 | 350 | z |
| `fixture/captured/ce5fd7b3efab1454.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/d1265e2b979af77f.step.gz` | 0 | 30 | 70 | z |
| `fixture/captured/d15574a5c527a905.step.gz` | 0 | 3000 | 5100 | z |
| `fixture/captured/d43a4ab2d4b92233.step.gz` | 0 | 150 | 350 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/d6245cf53278e216.step.gz` | 0 | 30 | 51 | z |
| `fixture/captured/d7ccf94127c3a549.step.gz` | 0 | 30 | 70 | x |
| `fixture/captured/d7ccf94127c3a549.step.gz` | 1 | 30 | 70 | x |
| `fixture/captured/d969998863570052.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/d9885689394340c0.step.gz` | 0 | 30 | 70 | x |
| `fixture/captured/d9a4dc264fe391c8.step.gz` | 0 | 30 | 73 | z |
| `fixture/captured/dac73da9ba85ba08.step.gz` | 0 | 10 | 8 | z |
| `fixture/captured/dac73da9ba85ba08.step.gz` | 1 | 14 | 12 | z |
| `fixture/captured/dac73da9ba85ba08.step.gz` | 2 | 18 | 16 | z |
| `fixture/captured/dd66f3833d20a117.step.gz` | 0 | 40 | 80 | y |
| `fixture/captured/e2f5c1981d76f9a1.step.gz` | 0 | 16 | 10 | z |
| `fixture/captured/e2f5c1981d76f9a1.step.gz` | 1 | 10 | 8 | z |
| `fixture/captured/e3b775d03c15ab72.step.gz` | 0 | 40 | 50 | z |
| `fixture/captured/e477b629f5858031.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/e6e84b1480b07b2a.step.gz` | 0 | 30 | 42.5 | z |
| `fixture/captured/e7e55b939aebc230.step.gz` | 0 | 60 | 40 | z |
| `fixture/captured/e8c5d34a39d57ed7.step.gz` | 0 | 30 | 70 | x |
| `fixture/captured/e8c5d34a39d57ed7.step.gz` | 1 | 30 | 70 | x |
| `fixture/captured/eaa547858ffda899.step.gz` | 0 | 30 | 60 | y |
| `fixture/captured/eea1b44e77612c35.step.gz` | 0 | 20 | 20 | z |
| `fixture/captured/ef5fba9a2b73897b.step.gz` | 0 | 40 | 50 | z |
| `fixture/captured/effective_surfaces/parts/4a705fc57e02c7bb.step.gz` | 0 | 24 | 4 | z |
| `fixture/captured/effective_surfaces/parts/57280be01b58b1f3.step.gz` | 0 | 8 | 10 | z |
| `fixture/captured/f059038539ab738d.step.gz` | 0 | 40 | 100 | z |
| `fixture/captured/f2bd461093efbb24.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/f348a103e64a9c34.step.gz` | 0 | 40 | 80 | z |
| `fixture/captured/f348a103e64a9c34.step.gz` | 1 | 40 | 80 | z |
| `fixture/captured/f3e0d1bbcf335b03.step.gz` | 0 | 8 | 10 | z |
| `fixture/captured/f4c93b1c9a48ce65.step.gz` | 0 | 20 | 40 | x |
| `fixture/captured/f4fdfbdd88715ecd.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/f5225cd3ba76f1d2.step.gz` | 0 | 40 | 10 | z |
| `fixture/captured/f8f9e12fa57c9ab7.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/f96812b6fb223d9d.step.gz` | 0 | 15 | 40 | z |
| `fixture/captured/frames/cylinder_rot_x37.step` | 0 | 20 | 30 | [0, -0.601815023, 0.79863551] |
| `fixture/captured/passages/0a3141da4c6d054d.step.gz` | 0 | 30 | 70 | z |
| `fixture/captured/passages/ce379caf6a4f0096.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/passages/eb339e6d45fbdde0.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/section_recess_helpers/parts/9f246ea373b32179.step.gz` | 0 | 14.3 | 6 | y |
| `fixture/captured/sections/parts/294e40cd0cf7b180.step.gz` | 0 | 30 | 70 | z |
| `fixture/captured/sections/parts/8caed4acacd0ea95.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/captured/sections/parts/9c84ae523330ac20.step.gz` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/filleted_stepped_shaft.step` | 0 | 30 | 70 | z |
| `fixture/full_torus.step` | 0 | 24 | 4 | z |
| `fixture/golden_toroidal_blends_turned.step` | 0 | 30 | 70 | [0.390731128, 0, 0.920504853] |
| `fixture/golden_turned_steps_and_grooves.step` | 0 | 30 | 70 | z |
| `fixture/hemisphere_one_edge.step` | 0 | 25.4 | 12.7 | y |
| `fixture/mixed.step` | 0 | 30 | 70 | z |
| `fixture/rejected_cylinder.step` | 0 | 20 | 20 | z |
| `fixture/slanted_bead.step` | 0 | 24 | 20 | [-0, 0.707106781, -0.707106781] |
| `fixture/spotface_outer_steps.step` | 0 | 40 | 12 | z |
| `fixture/stack.step` | 0 | 10 | 19 | z |
| `fixture/ties/symmetric_cylinder.step` | 0 | 10 | 20 | z |
| `fixture/toroidal_bead.step` | 0 | 24 | 20 | z |
| `fixture/turned.step` | 0 | 30 | 70 | z |
| `fixture/turned_body_disc.step` | 0 | 22 | 7 | z |
| `fixture/turned_rot37.step` | 0 | 30 | 70 | [0, -0.601815023, 0.79863551] |
| `fixture/turned_with_sphere.step` | 0 | 30 | 73 | z |

## Every OD-boss suppression (raw geometric evidence)

| Part | Faces | Diameter = OD | Span | Body length |
|---|---|---:|---|---:|
| `repro/bearing-608zz.step` | [3] | 22 | [-3.25, 3.25] (6.5) | 7 |
| `fixture/annular_face_groove.step` | [0] | 24 | [-5, 5] (10) | 10 |
| `fixture/captured/2733401823bf7439.step.gz` | [6] | 24 | [-4, 4] (8) | 8 |
| `fixture/captured/2733401823bf7439.step.gz` | [15] | 24 | [-4, 4] (8) | 8 |
| `fixture/captured/2c8ff38cf754d67e.step.gz` | [0] | 10 | [-5, 5] (10) | 10 |
| `fixture/captured/2c8ff38cf754d67e.step.gz` | [3] | 10 | [-5, 5] (10) | 10 |
| `fixture/captured/3aa8de56630d5a8f.step.gz` | [0] | 30 | [-10, 10] (20) | 20 |
| `fixture/captured/3aa8de56630d5a8f.step.gz` | [3] | 24 | [-2.5, 2.5] (5) | 5 |
| `fixture/captured/3aa8de56630d5a8f.step.gz` | [6] | 30 | [-10, 10] (20) | 20 |
| `fixture/captured/4a8a743698bc98d6.step.gz` | [0] | 30 | [-20, 20] (40) | 40 |
| `fixture/captured/93bc8ca0db4db566.step.gz` | [0, 5] | 300 | [-21.25, 21.25] (42.5) | 42.5 |
| `fixture/captured/dac73da9ba85ba08.step.gz` | [0] | 10 | [-4, 4] (8) | 8 |
| `fixture/captured/dac73da9ba85ba08.step.gz` | [3] | 14 | [-6, 6] (12) | 12 |
| `fixture/captured/dac73da9ba85ba08.step.gz` | [6] | 18 | [-8, 8] (16) | 16 |
| `fixture/captured/e2f5c1981d76f9a1.step.gz` | [0] | 16 | [-5, 5] (10) | 10 |
| `fixture/captured/e2f5c1981d76f9a1.step.gz` | [3] | 10 | [-4, 4] (8) | 8 |
| `fixture/captured/e7e55b939aebc230.step.gz` | [0] | 60 | [-20, 20] (40) | 40 |
| `fixture/captured/eea1b44e77612c35.step.gz` | [0] | 20 | [-10, 10] (20) | 20 |
| `fixture/captured/effective_surfaces/parts/57280be01b58b1f3.step.gz` | [0] | 8 | [-5, 5] (10) | 10 |
| `fixture/captured/f3e0d1bbcf335b03.step.gz` | [0] | 8 | [-5, 5] (10) | 10 |
| `fixture/captured/f5225cd3ba76f1d2.step.gz` | [0] | 40 | [-5, 5] (10) | 10 |
| `fixture/captured/frames/cylinder_rot_x37.step` | [0] | 20 | [-15, 15] (30) | 30 |
| `fixture/captured/section_recess_helpers/parts/9f246ea373b32179.step.gz` | [0] | 14.3 | [-3, 3] (6) | 6 |
| `fixture/rejected_cylinder.step` | [0] | 20 | [-10, 10] (20) | 20 |
| `fixture/spotface_outer_steps.step` | [0] | 40 | [-6, 6] (12) | 12 |
| `fixture/ties/symmetric_cylinder.step` | [0] | 10 | [-10, 10] (20) | 20 |
| `fixture/turned_body_disc.step` | [1] | 22 | [-3.25, 3.25] (6.5) | 7 |

## Turned-step inputs that are not turned bodies

| Part | Incompatible support witness |
|---|---|
| `corpus/cadgenbench/flanged_spool_132.step` | Face 4 Cone; axis offset 77.9422863, sine of angle 0 against the first cylinder axis |
| `corpus/cadgenbench/threaded_connector_109.step` | Face 17 Plane; axis offset 51.5418184, sine of angle 0.999807953 against the first cylinder axis |
| `corpus/cadgenbench_inputs/cgb217.step.gz` | Face 4 Torus; axis offset 32.305998, sine of angle 0 against the first cylinder axis |
| `corpus/gramel/string_post.step` | Face 5 Cylinder; axis offset 4.75, sine of angle 1 against the first cylinder axis |
| `corpus/nist/nist_ctc_05_asme1_rd.stp` | Face 2 Plane; axis offset 279.4, sine of angle 1 against the first cylinder axis |
| `fixture/ap242/specify/spool_fits.step.gz` | Face 4 Cone; axis offset 77.9422863, sine of angle 0 against the first cylinder axis |
| `fixture/ap242/specify/string_post_tapped.step.gz` | Face 5 Cylinder; axis offset 4.75, sine of angle 1 against the first cylinder axis |
| `fixture/ap242/write/datum_targets.step.gz` | Face 4 Cone; axis offset 77.9422863, sine of angle 0 against the first cylinder axis |
| `fixture/ap242/write/datum_targets_shared_support.step.gz` | Face 4 Cone; axis offset 77.9422863, sine of angle 0 against the first cylinder axis |
| `fixture/ap242/write/decimal_default.step.gz` | Face 4 Cone; axis offset 77.9422863, sine of angle 0 against the first cylinder axis |
| `fixture/ap242/write/every_kind.step.gz` | Face 4 Cone; axis offset 77.9422863, sine of angle 0 against the first cylinder axis |
| `fixture/ap242/write/pmi_gaps.step.gz` | Face 4 Cone; axis offset 77.9422863, sine of angle 0 against the first cylinder axis |
| `fixture/ap242/write/pmi_gaps_gram.step.gz` | Face 4 Cone; axis offset 77.9422863, sine of angle 0 against the first cylinder axis |
| `fixture/ap242/write/spool_fits_intent.step.gz` | Face 4 Cone; axis offset 77.9422863, sine of angle 0 against the first cylinder axis |
| `fixture/ap242/write/string_post_tapped_intent.step.gz` | Face 5 Cylinder; axis offset 4.75, sine of angle 1 against the first cylinder axis |
| `fixture/ap242/write/thread_removed.step.gz` | Face 4 Cone; axis offset 77.9422863, sine of angle 0 against the first cylinder axis |
| `fixture/ap242/write/thread_replaced.step.gz` | Face 4 Cone; axis offset 77.9422863, sine of angle 0 against the first cylinder axis |
| `fixture/captured/06d769dd5e6a360e.step.gz` | Face 4 Plane; axis offset 1.41421356, sine of angle 1 against the first cylinder axis |
| `fixture/captured/49f41c718be3fb86.step.gz` | Face 4 Cone; axis offset 77.9422863, sine of angle 0 against the first cylinder axis |

## Baseline reader refusals

These are byte-for-byte unchanged errors from main; they were audited as refusals, not silently omitted.

- `fixture/ap242/removal/handmade.stp`: incomplete STEP geometry: no solid or shell with faces
- `fixture/ap242/rules/invalid.stp`: incomplete STEP geometry: no solid or shell with faces
- `fixture/ap242/rules/valid.stp`: incomplete STEP geometry: no solid or shell with faces
- `fixture/broken/deleted_face.step`: incomplete STEP geometry: #109 is referenced but not in the file (3 shape entities lost)
- `fixture/broken/empty_data.step`: incomplete STEP geometry: no solid or shell with faces
- `fixture/broken/header_only.step`: STEP parse error: parse error: unexpected end of file, expected any token
- `fixture/broken/truck_pub_shaft_hole.step.gz`: STEP parse error: parse error at line 639, column 1: duplicate entity #539
- `fixture/broken/truncated.step`: STEP parse error: parse error: unexpected end of file, expected any token
- `fixture/broken/truncated_closed.step`: incomplete STEP geometry: #105 is referenced but not in the file; #109 is referenced but not in the file (3 shape entities lost)
- `fixture/broken/unresolved_closed_curve.step`: unsupported STEP content: a closed edge's curve (Hyperbola) does not resolve
- `fixture/occt_trimmed_parabola.step`: incomplete STEP geometry: no solid or shell with faces

## Validation and re-recording

Pending completion of the full pinned-corpus snapshot producers and byte-for-byte check, workspace nextest run, formatting and clippy. Additive inventory floors name #123 and retain every historical feature baseline.
