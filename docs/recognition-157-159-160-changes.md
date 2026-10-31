# Recognition #157, #159, #160: local review record

All source and tests are committed on `cx/recog-157-159-160`. No push or external post was made. Recognition `/6`, fingerprints `/3`, and crate versions stay unchanged. No web family was added; no file under `crates/haecceity` was edited.

The official snapshot tool was run once in record mode at `5afb784e06779f16ba5bd069652fd12b0f1ef1b8`, then in check mode with the required pinned corpus. All 2,700 compressed snapshots reproduce byte for byte; seven payloads changed and 2,693 did not. Provenance is the eighth changed snapshot-directory file.

The [complete field audit](recognition-157-159-160-audit.json.gz) contains all 12,308 changed JSON paths with snapshot, part, old value, new value, issue and reason. Decode with `gzip -dc docs/recognition-157-159-160-audit.json.gz`. The [logical record manifest](recognition-157-159-160-records.json) lists all 64 mergers, 807 ID changes and 56 corpus corner omissions without conflating renumbering with remeasurement.

## For #157

The flats make the head section 24 × 20 mm: its 16.67% departure from square exceeds the old 15% step gate. Separately, whole-body classification refused the lateral planes. The exception requires independently proved stock flats parallel to the common revolution axis at model precision; every remaining support must share that axis; complete external coaxial cylinder bands must cover more than half the body length, counting overlapping spans once. OD fill remains required. The spindle has 40/52 mm of complete bands. The D-section extrusion, prisms and an offset bore-axis negative remain refused.

Plain: 3 steps → 3. With flats: 0 → 3 steps, 0 → 1 turned body, retaining 2 flats across 20 mm on stock span [0,12]. Diameters/lengths are 24×12, 15×22 and 10×18 mm. Shoulder faces 5 and 7 are `plane_shoulder` segments and defining faces of `turned_bodies/0`; they are explained.

The plain plate is a real 12 mm head slab satisfying the existing thickness and skin-coverage guards. The riser is its shoulder viewed in the inferred measuring frame: `height_axis: "y"` makes `z_lo: -12` the radial Y extent, not file Z. Both descriptions remain. No existing corpus turning, boss, plate or riser record changes; only the two new spindle input hashes enter provenance.

## For #159

Native torus faces previously produced independent records. Grouping requires one valid solid, equal raw torus centres, axis lines, major/minor radii, material side and meridian interval at model precision. One existing Fillet record now carries every constituent arc; its anchor remains an actual source-arc anchor. This uses the existing multi-face `/6` contract with no fields or families added.

The hub has 6 → 4 total fillets: root records on [9], [13], [5] become one R3 concave turned fillet with exact defining faces [5,9,13]. The three other rounds remain.

| Corpus part | Fillet records old → new | New multi-arc groups |
|---|---:|---:|
| cadgenbench_inputs/cgb202.step.gz | 708 → 542 | 35 |
| cadgenbench_inputs/cgb207.step | 74 → 73 | 1 |
| cadgenbench_inputs/cgb217.step.gz | 85 → 84 | 1 |
| cadgenbench_inputs/cgb242.step.gz | 241 → 209 | 21 |
| cadgenbench_inputs/cgb243.step.gz | 49 → 45 | 3 |
| nist/nist_ctc_05_asme1_rd.stp | 23 → 20 | 3 |

Every old fillet face is retained on all six parts. Each merged record copies the parameters of one original member; unchanged logical records retain their parameters. Only defining-set aggregation and run-local IDs change. Physical face fingerprints are unchanged across all 100 corpus parts. Historical inventory values and loss allowances were not lowered: the census counts native torus patches within each group, with cylindrical rounds (including spherical caps) still counted once. The original cgb202 floor remains 708.

Two related effects are retained and fully listed in the field audit:

- cgb202 `bosses/12.neighbourhood`: 60 → 137 entries, adding 77 `cylinder>out:plane:convex` tokens. Existing contraction follows all defining arcs of the now-single convex treatment to their transverse supports. This is grouped supporting-graph context; boss dimensions, defining faces, placement and other fingerprint fields are unchanged.
- The legacy degradation questions for [698,2091,2092] and [1627,2091,2139] read `published` → `skipped`. Their subset/superset lookup no longer matches the enlarged defining lists. Arc 698 remains in [621,632,642,698,703,724]; arc 1627 remains in [1612,1614,1617,1618,1620,1621,1624,1626,1627]. These labels do not signify loss of the arcs.

All record IDs, faces and old → new fingerprint/accounting/reconciliation/retry fields are enumerated in the compressed audit. The logical manifest includes the 807 unchanged-shape fillet ID mappings.

### Every merged corpus record

| Part | Old records and defining faces → new record and defining faces | Reason |
|---|---|---|
| cadgenbench_inputs/cgb202.step.gz | `fillets/30:[2108]; fillets/31:[2109]; fillets/33:[2104]; fillets/35:[2100]; fillets/38:[2095]; fillets/39:[2086]` → `fillets/30:[2086,2095,2100,2104,2108,2109]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/32:[2063]; fillets/34:[2066]; fillets/36:[2069]; fillets/37:[2072]` → `fillets/31:[2063,2066,2069,2072]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/50:[632]; fillets/51:[642]; fillets/55:[724]; fillets/58:[621]; fillets/61:[703]; fillets/63:[698]` → `fillets/42:[621,632,642,698,703,724]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/52:[667]; fillets/56:[672]; fillets/59:[677]; fillets/60:[682]` → `fillets/43:[667,672,677,682]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/93:[26]; fillets/95:[714]; fillets/97:[25]; fillets/100:[21]; fillets/104:[16]` → `fillets/77:[16,21,25,26,714]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/94:[30]; fillets/96:[32]; fillets/98:[33]; fillets/101:[34]; fillets/103:[0]` → `fillets/78:[0,30,32,33,34]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/141:[1621]; fillets/142:[1620]; fillets/143:[1618]; fillets/144:[1617]; fillets/145:[1612]; fillets/146:[1614]; fillets/147:[1626]; fillets/148:[1624]; fillets/149:[1627]` → `fillets/117:[1612,1614,1617,1618,1620,1621,1624,1626,1627]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/150:[176]; fillets/151:[172]; fillets/152:[221]; fillets/153:[283]; fillets/154:[291]; fillets/155:[299]; fillets/156:[355]; fillets/157:[363]` → `fillets/118:[172,176,221,283,291,299,355,363]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/159:[785]; fillets/162:[778]; fillets/163:[842]` → `fillets/120:[778,785,842]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/160:[1102]; fillets/161:[1116]; fillets/165:[1096]; fillets/170:[1073]; fillets/174:[1083]; fillets/179:[798]; fillets/183:[818]; fillets/184:[804]` → `fillets/121:[798,804,818,1073,1083,1096,1102,1116]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/164:[1117]; fillets/172:[791]; fillets/180:[792]` → `fillets/122:[791,792,1117]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/166:[736]; fillets/167:[68]; fillets/168:[1203]; fillets/169:[50]; fillets/171:[56]` → `fillets/123:[50,56,68,736,1203]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/173:[61]; fillets/175:[745]; fillets/176:[1194]; fillets/177:[73]; fillets/178:[755]` → `fillets/124:[61,73,745,755,1194]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/181:[950]; fillets/182:[764]; fillets/185:[771]` → `fillets/125:[764,771,950]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/240:[1228]; fillets/241:[1226]; fillets/242:[1229]; fillets/243:[1227]; fillets/244:[1236]; fillets/245:[1230]; fillets/246:[1235]; fillets/247:[1232]; fillets/248:[1234]; fillets/249:[1231]; fillets/250:[1233]` → `fillets/180:[1226,1227,1228,1229,1230,1231,1232,1233,1234,1235,1236]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/303:[181]; fillets/304:[169]; fillets/305:[216]; fillets/306:[286]; fillets/307:[294]; fillets/308:[302]; fillets/309:[358]; fillets/310:[366]` → `fillets/233:[169,181,216,286,294,302,358,366]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/311:[1509]; fillets/312:[1516]` → `fillets/234:[1509,1516]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/313:[184]; fillets/314:[164]; fillets/315:[213]; fillets/316:[267]; fillets/317:[275]; fillets/318:[323]; fillets/319:[347]; fillets/320:[371]` → `fillets/235:[164,184,213,267,275,323,347,371]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/321:[1988]; fillets/322:[1981]; fillets/323:[2025]; fillets/324:[1974]; fillets/325:[2040]; fillets/326:[2017]; fillets/327:[2031]; fillets/328:[1995]; fillets/329:[1954]; fillets/330:[2002]; fillets/331:[2009]` → `fillets/236:[1954,1974,1981,1988,1995,2002,2009,2017,2025,2031,2040]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/332:[1181]; fillets/333:[1180]; fillets/334:[1179]; fillets/335:[1182]; fillets/336:[1177]; fillets/337:[1176]; fillets/338:[1178]; fillets/339:[1183]` → `fillets/237:[1176,1177,1178,1179,1180,1181,1182,1183]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/357:[1733]; fillets/358:[1732]` → `fillets/255:[1732,1733]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/376:[189]; fillets/377:[161]; fillets/378:[208]; fillets/379:[270]; fillets/380:[278]; fillets/381:[326]; fillets/382:[350]; fillets/383:[374]` → `fillets/273:[161,189,208,270,278,326,350,374]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/384:[197]; fillets/385:[156]; fillets/386:[205]; fillets/387:[233]; fillets/388:[259]; fillets/389:[307]; fillets/390:[339]; fillets/391:[379]` → `fillets/274:[156,197,205,233,259,307,339,379]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/414:[192]; fillets/415:[153]; fillets/416:[200]; fillets/417:[236]; fillets/418:[262]; fillets/419:[310]; fillets/420:[342]; fillets/421:[382]` → `fillets/297:[153,192,200,236,262,310,342,382]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/422:[148]; fillets/423:[114]; fillets/424:[140]; fillets/425:[225]; fillets/426:[251]; fillets/427:[315]; fillets/428:[331]; fillets/429:[387]` → `fillets/298:[114,140,148,225,251,315,331,387]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/446:[145]; fillets/447:[111]; fillets/448:[137]; fillets/449:[228]; fillets/450:[254]; fillets/451:[318]; fillets/452:[334]; fillets/453:[390]` → `fillets/315:[111,137,145,228,254,318,334,390]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/454:[130]; fillets/455:[120]; fillets/456:[104]; fillets/457:[243]; fillets/458:[95]; fillets/459:[426]; fillets/460:[420]; fillets/461:[410]` → `fillets/316:[95,104,120,130,243,410,420,426]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/477:[1378]; fillets/478:[1371]; fillets/479:[1367]; fillets/480:[1366]; fillets/481:[1362]` → `fillets/332:[1362,1366,1367,1371,1378]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/482:[126]; fillets/483:[124]` → `fillets/333:[124,126]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/484:[1375]; fillets/485:[1376]; fillets/487:[1373]; fillets/488:[1369]; fillets/489:[1364]; fillets/490:[1360]; fillets/492:[1335]; fillets/494:[1352]; fillets/496:[1357]; fillets/498:[1356]` → `fillets/334:[1335,1352,1356,1357,1360,1364,1369,1373,1375,1376]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/493:[402]; fillets/495:[406]; fillets/497:[404]` → `fillets/337:[402,404,406]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/499:[416]; fillets/500:[414]` → `fillets/338:[414,416]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/502:[1427]; fillets/503:[1431]` → `fillets/340:[1427,1431]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/504:[1433]; fillets/505:[1436]; fillets/507:[1442]` → `fillets/341:[1433,1436,1442]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb202.step.gz | `fillets/506:[1438]; fillets/508:[1444]` → `fillets/342:[1438,1444]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb207.step | `fillets/26:[133]; fillets/27:[135]` → `fillets/26:[133,135]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb217.step.gz | `fillets/45:[4]; fillets/47:[8]` → `fillets/45:[4,8]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/0:[802]; fillets/1:[821]; fillets/2:[789]` → `fillets/0:[789,802,821]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/3:[623]; fillets/8:[619]` → `fillets/1:[619,623]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/4:[620]; fillets/6:[626]` → `fillets/2:[620,626]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/5:[602]; fillets/10:[591]` → `fillets/3:[591,602]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/11:[119]; fillets/17:[382]` → `fillets/6:[119,382]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/12:[613]; fillets/15:[580]; fillets/16:[111]` → `fillets/7:[111,580,613]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/13:[631]; fillets/18:[644]` → `fillets/8:[631,644]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/21:[649]; fillets/22:[633]; fillets/23:[25]` → `fillets/12:[25,633,649]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/26:[345]; fillets/27:[120]` → `fillets/15:[120,345]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/54:[62]; fillets/56:[838]; fillets/58:[845]` → `fillets/42:[62,838,845]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/55:[854]; fillets/57:[864]` → `fillets/43:[854,864]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/59:[874]; fillets/62:[867]` → `fillets/44:[867,874]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/60:[106]; fillets/61:[572]` → `fillets/45:[106,572]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/63:[28]; fillets/64:[658]; fillets/65:[661]` → `fillets/46:[28,658,661]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/66:[844]; fillets/68:[60]` → `fillets/47:[60,844]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/74:[13]; fillets/75:[14]; fillets/76:[668]` → `fillets/54:[13,14,668]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/94:[384]; fillets/95:[153]` → `fillets/72:[153,384]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/138:[127]; fillets/144:[740]; fillets/154:[731]` → `fillets/115:[127,731,740]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/139:[98]; fillets/147:[754]; fillets/157:[745]` → `fillets/116:[98,745,754]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/145:[720]; fillets/146:[124]; fillets/155:[727]; fillets/156:[724]` → `fillets/121:[124,720,724,727]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb242.step.gz | `fillets/148:[716]; fillets/153:[94]; fillets/158:[712]` → `fillets/122:[94,712,716]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb243.step.gz | `fillets/0:[49]; fillets/1:[409]; fillets/2:[393]` → `fillets/0:[49,393,409]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb243.step.gz | `fillets/20:[370]; fillets/23:[369]` → `fillets/18:[369,370]` | Same proved torus and meridian; one callout. |
| cadgenbench_inputs/cgb243.step.gz | `fillets/27:[389]; fillets/28:[53]` → `fillets/24:[53,389]` | Same proved torus and meridian; one callout. |
| nist/nist_ctc_05_asme1_rd.stp | `fillets/0:[174]; fillets/1:[73]` → `fillets/0:[73,174]` | Same proved torus and meridian; one callout. |
| nist/nist_ctc_05_asme1_rd.stp | `fillets/6:[9]; fillets/7:[80]` → `fillets/5:[9,80]` | Same proved torus and meridian; one callout. |
| nist/nist_ctc_05_asme1_rd.stp | `fillets/9:[126]; fillets/10:[127]` → `fillets/7:[126,127]` | Same proved torus and meridian; one callout. |

## For #160

A face centroid need not be the midpoint of the bevel section. Nearest support planes and one point probe did not establish a reconstructible, empty wedge. Retained principal-run corners now reproduce published legs within 0.005 mm and angle within 0.05°, and require a complete zero-material wedge proof. Failure leaves `corner: null`; the feature, legs and angle remain. Rolled support corners are also omitted when coordinate-axis reconstruction is unproved.

The supplied OCCT/build123d check, changed only to skip null corners, gives 9/41 failing → 0/41 failing: 31 corners omitted and 10 retained, including the three oblique records. The other 22 omissions in that set are conservative failures of the stronger production proof, not claims that the original loose check failed. The same proof removes 25 further corpus corners and 17 captured-call occurrences. The following table lists every corpus record and exact old → new value; the full audit lists each captured call by source STEP and JSON path. No other corpus record parameters change.

| Part | Record | `corner` old → new | Reason |
|---|---|---|---|
| cadgenbench_inputs/cgb243.step.gz | chamfers/2 | `[-15.632,48.868,-22.427]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| cadgenbench_inputs/cgb243.step.gz | chamfers/3 | `[-10.632,48.868,-22.427]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| cadgenbench_inputs/cgb243.step.gz | chamfers/14 | `[-13.132,33.868,-50.001]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| cadgenbench_inputs/cgb243.step.gz | chamfers/15 | `[-13.132,-36.132,-50.001]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| cadgenbench_inputs/cgb243.step.gz | chamfers/32 | `[-58.272,-1.132,-50.001]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| cadgenbench_inputs/cgb243.step.gz | chamfers/33 | `[-35.132,33.948,-50.001]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| cadgenbench_inputs/cgb243.step.gz | chamfers/34 | `[-35.132,-36.211,-50.001]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| cadgenbench_inputs/cgb243.step.gz | chamfers/35 | `[8.868,33.948,-50.001]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| cadgenbench_inputs/cgb243.step.gz | chamfers/36 | `[8.868,-36.211,-50.001]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| cadgenbench_inputs/cgb243.step.gz | chamfers/37 | `[32.009,-1.132,-50.001]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10000.step | angled_steps/0 | `[5.325,0.0,0.0]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10007.step | angled_steps/0 | `[31.864,0.0,8.887]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10020.step | chamfers/0 | `[0.0,5.819,17.961]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10033.step | chamfers/0 | `[0.0,17.182,0.0]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10049.step | angled_steps/0 | `[11.535,3.628,0.0]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10063.step | angled_steps/0 | `[7.867,5.834,0.0]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10077.step | chamfers/0 | `[23.623,34.653,11.577]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10077.step | chamfers/1 | `[19.313,0.0,13.526]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10077.step | chamfers/2 | `[23.623,23.537,0.0]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/1008.step | chamfers/0 | `[24.831,28.5,0.0]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10092.step | chamfers/0 | `[9.828,14.663,0.0]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10101.step | angled_steps/0 | `[13.154,10.355,9.404]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10103.step | chamfers/0 | `[9.974,13.928,5.109]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10119.step | angled_steps/0 | `[12.57,18.004,6.257]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10131.step | chamfers/0 | `[0.0,10.091,23.226]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10138.step | angled_steps/1 | `[0.0,0.0,17.713]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10146.step | chamfers/0 | `[5.003,24.223,0.0]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10163.step | chamfers/0 | `[0.0,0.0,8.327]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/1017.step | chamfers/1 | `[0.0,17.116,29.361]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10170.step | chamfers/0 | `[14.615,0.0,7.738]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10224.step | angled_steps/0 | `[31.4,45.895,14.771]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10245.step | angled_steps/0 | `[0.0,39.198,3.426]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp/10247.step | angled_steps/0 | `[17.981,0.0,0.0]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp_holdout/163.step | angled_steps/0 | `[3.641,0.0,0.0]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp_holdout/16943.step | angled_steps/0 | `[38.012,0.0,20.643]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp_holdout/181.step | angled_steps/0 | `[35.505,21.059,17.718]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp_holdout/340.step | angled_steps/0 | `[32.35,21.996,0.0]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp_holdout/408.step | angled_steps/0 | `[17.626,42.134,10.456]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp_holdout/419.step | chamfers/0 | `[22.32,0.0,23.22]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp_holdout/471.step | chamfers/0 | `[21.0,14.992,15.193]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp_holdout/483.step | angled_steps/0 | `[6.606,0.0,24.738]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp_regressions/11512.step | chamfers/0 | `[0.0,2.931,14.863]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| mfcadpp_regressions/11512.step | angled_steps/0 | `[25.468,7.153,8.096]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| nist/nist_ctc_01_asme1_rd.stp | chamfers/1 | `[50.0,0.0,50.0]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| nist/nist_ctc_01_asme1_rd.stp | chamfers/2 | `[-50.0,0.0,50.0]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| nist/nist_ctc_03_asme1_rc.stp | chamfers/2 | `[-241.3,485.445,0.954]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| nist/nist_ctc_03_asme1_rc.stp | chamfers/5 | `[-248.607,425.455,128.908]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| nist/nist_ftc_06_asme1_rd.stp | chamfers/2 | `[88.9,97.79,-15.875]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| nist/nist_ftc_06_asme1_rd.stp | chamfers/3 | `[76.2,97.79,-15.875]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| nist/nist_ftc_06_asme1_rd.stp | chamfers/4 | `[-76.2,97.79,-15.875]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| nist/nist_ftc_06_asme1_rd.stp | chamfers/5 | `[-88.9,97.79,-15.875]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| nist/nist_ftc_06_asme1_rd.stp | chamfers/6 | `[0.0,97.79,-133.35]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| nist/nist_ftc_09_asme1_rd.stp | chamfers/0 | `[95.25,1.519,-117.475]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| nist/nist_ftc_09_asme1_rd.stp | chamfers/2 | `[74.612,1.519,-139.7]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| nist/nist_ftc_09_asme1_rd.stp | chamfers/4 | `[-74.612,1.519,-139.7]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |
| nist/nist_ftc_09_asme1_rd.stp | chamfers/6 | `[-95.25,1.519,-117.475]` → `null` | Published-coordinate / complete empty-wedge proof did not succeed. |

### Captured-call occurrences

| Source STEP | Snapshot and field | `corner` old → new | Reason |
|---|---|---|---|
| f71fc920508b4dc9.step.gz | `calls/recognise_angled_steps.json.gz/68/result/0/corner` | `[-17.5,20.0,6.0]` → `null` | Same conservative corner proof. |
| f71fc920508b4dc9.step.gz | `calls/recognise_angled_steps.json.gz/69/result/0/corner` | `[-17.5,20.0,6.0]` → `null` | Same conservative corner proof. |
| ad6bc5f05b2a7de7.step.gz | `calls/recognise_chamfers.json.gz/37/result/0/corner` | `[0.0,-100.0,-30.0]` → `null` | Same conservative corner proof. |
| ad6bc5f05b2a7de7.step.gz | `calls/recognise_chamfers.json.gz/37/result/1/corner` | `[0.0,100.0,-30.0]` → `null` | Same conservative corner proof. |
| ad6bc5f05b2a7de7.step.gz | `calls/recognise_chamfers.json.gz/37/result/2/corner` | `[-100.0,0.0,-30.0]` → `null` | Same conservative corner proof. |
| ad6bc5f05b2a7de7.step.gz | `calls/recognise_chamfers.json.gz/37/result/3/corner` | `[100.0,0.0,-30.0]` → `null` | Same conservative corner proof. |
| e458fbb5794a4f78.step.gz | `calls/recognise_chamfers.json.gz/38/result/0/corner` | `[0.0,-40.0,-30.0]` → `null` | Same conservative corner proof. |
| e458fbb5794a4f78.step.gz | `calls/recognise_chamfers.json.gz/38/result/1/corner` | `[0.0,40.0,-30.0]` → `null` | Same conservative corner proof. |
| e458fbb5794a4f78.step.gz | `calls/recognise_chamfers.json.gz/38/result/2/corner` | `[-40.0,0.0,-30.0]` → `null` | Same conservative corner proof. |
| e458fbb5794a4f78.step.gz | `calls/recognise_chamfers.json.gz/38/result/3/corner` | `[40.0,0.0,-30.0]` → `null` | Same conservative corner proof. |
| ad6bc5f05b2a7de7.step.gz | `calls/recognise_chamfers.json.gz/39/result/0/corner` | `[0.0,-100.0,-30.0]` → `null` | Same conservative corner proof. |
| ad6bc5f05b2a7de7.step.gz | `calls/recognise_chamfers.json.gz/39/result/1/corner` | `[0.0,100.0,-30.0]` → `null` | Same conservative corner proof. |
| ad6bc5f05b2a7de7.step.gz | `calls/recognise_chamfers.json.gz/39/result/2/corner` | `[-100.0,0.0,-30.0]` → `null` | Same conservative corner proof. |
| ad6bc5f05b2a7de7.step.gz | `calls/recognise_chamfers.json.gz/39/result/3/corner` | `[100.0,0.0,-30.0]` → `null` | Same conservative corner proof. |
| 2db994ba714cc27f.step.gz | `calls/recognise_chamfers.json.gz/50/result/0/corner` | `[0.0,-20.0,6.0]` → `null` | Same conservative corner proof. |
| d088eacbf676741d.step.gz | `calls/recognise_chamfers.json.gz/68/result/0/corner` | `[-3000.0,0.0,600.0]` → `null` | Same conservative corner proof. |
| 27d20eebb46b631d.step.gz | `calls/recognise_chamfers.json.gz/70/result/0/corner` | `[-1.5,0.0,0.3]` → `null` | Same conservative corner proof. |

## Changed snapshot files

| File | Explanation |
|---|---|
| `corpus.json.gz` | #159 groups, IDs, fingerprints and accounting; #160 56 corner omissions. |
| `recognition_evidence.json.gz` | #159 grouped defining/constituent faces and corresponding candidate indices. |
| `reconcile.json.gz` | #159 fillet candidate/accepted lists and index references. |
| `retry_publication.json.gz` | #159 same grouped public records and their IDs in retry views. |
| `degradation_questions.json.gz` | #159 the two legacy query-membership labels described above. |
| `calls/recognise_chamfers.json.gz` | #160 15 null corner occurrences. |
| `calls/recognise_angled_steps.json.gz` | #160 2 null corner occurrences. |
| `provenance.json` | Official source hash, three added STEP hashes, seven changed gzip hashes; exact values in audit. |

## Instructions and checks

The [performance attempts](recognition-157-159-160-performance.json) record origin/main `454f35c8` versus this branch, release/default-feature builds, three alternating pairs on each of `mfcadpp/10020.step`, `nist/nist_ctc_03_asme1_rc.stp` and `ttt_inputs/sm-hanger.step`. All 18 `/usr/bin/time -l` attempts failed to provide instruction counts: `time: sysctl kern.clockrate: Operation not permitted`. No elapsed-time substitute or instruction regression percentage is claimed. This is the only blocked requested measurement.

Passed: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --release -- -D warnings`; affected turning, fillet, chamfer, bevel, angled-step and tie tests (old count/required-corner expectations updated and rerun); all 31 root recognition tests including all twelve motion slices; historical inventory census; supplied OCCT check (0/41); official record once; official `--check` with mandatory pinned corpus (all 2,700 byte-identical). One test binary was run at a time for the broad suites.
