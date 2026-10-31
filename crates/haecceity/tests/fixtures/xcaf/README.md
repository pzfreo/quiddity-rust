# Outside-producer PMI fixture

Provided with the #142 fix-round-1 brief: xcaf-pmi converted cadgen’s
`pmi_plate.xbf` into `ours.pmi.json`, imported onto `pmi_plate.geom.step`.
The semantic data matches cadgen’s own AP242 on the same faces.

The committed JSON migrates the draft’s index references to the settled v1
id-reference contract; it retains caller requirement IDs and OCCT face-name
anchors. The geometry is the supplied STEP, losslessly gzip-compressed.
