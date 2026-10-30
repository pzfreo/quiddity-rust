# PMI JSON reference

## Standards check — 2026-10-30 (issue #142, rule 6 / agreement g)

[ISO 10303-28:2007](https://www.iso.org/standard/40646.html) specifies XML
representations of EXPRESS schemas and data, including EXPRESS-to-XSD mapping;
it is not a JSON binding. Its XML exchange structure is an entity graph.

AP242 edition 4 includes the edition 5 domain model, ISO/TS 10303-4442:2025.
The [ISO domain-model XSD location](https://standards.iso.org/iso/ts/10303/-4442/ed-5/tech/xml-schema/domain_model/)
and [MBx-IF ed4 Domain Model XML practice](https://www.mbx-if.org/home/wp-content/uploads/2025/10/rec_prac_ap242xml_assy_struct_v4.0.pdf)
identify XML as the implemented domain-model binding. The
[AP242 ed4 project](https://atlas.afnet.fr/en/iso-step-ap242-edition-4/)
also lists web services using JSON. That is evidence of JSON work, not enough to
claim an available normative JSON encoding of all EXPRESS or semantic PMI.

A public [experimental EXPRESS JSON encoding, v1.0.0.0, 2026-03-10](https://spatialcompute3d.com/blogs/express_json/express_json.html)
exists. It maps Part 21 entity instances and SELECTs to JSON arrays. It calls
itself experimental and is not an ISO publication or an identified ISO draft.
Searches of ISO's public catalogue, the AP242 project and MBx-IF publications
found no published or publicly accessible draft ISO JSON binding with rules that
could govern this mapping. This is a bounded finding, not a claim that no
unpublished work exists. In particular, ed4 JSON web-service work must not be
confused with a published Part 28 JSON replacement.

We therefore follow the existing AP242 EXPRESS semantic model and
[CAx-IF PMI Representation and Presentation v4.1, 2024-06-20](https://www.mbx-if.org/home/wp-content/uploads/2024/06/rec_pracs_pmi_v41.pdf).
This is an application-level semantic JSON mapping, not ISO Part 28 conformance
or a Part 21 entity-graph serialization. It retains stated decimal tokens,
AP242 names and the practice's groupings. Supplemental geometry alone retains
its typed geometry tree because that is how `PartPmi` represents it.
