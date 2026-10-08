"""Generate crates/haecceity/src/express_table.rs from AP242 EXPRESS long forms.

Usage (python3 standard library only; run with -I):

    python3 -I tools/express_table.py ED1.exp ED4.exp > crates/haecceity/src/express_table.rs

ED1.exp is stepcode's `data/ap242/242_n8324_mim_lf.exp` (WG12 N8324, AP242 edition 1) and ED4.exp
its `data/ap242/242_mim_lf.exp` (WG12 N11521, AP242 edition 4); the source URLs and the files'
sha256 are written into the output and the test checks them. The output is deterministic: a
rerun on the same inputs reproduces it byte for byte.

What is read: TYPE (SELECT, ENUMERATION, simple and aggregate defined types, nested) and ENTITY
declarations (ABSTRACT, SUPERTYPE OF with ONEOF / ANDOR / AND, SUBTYPE OF, explicit attributes with
OPTIONAL, aggregates with their bounds, explicit redeclarations `SELF\\e.a : t`, DERIVE clauses
that redeclare an inherited explicit attribute, and the labels of UNIQUE and WHERE rules). INVERSE
attributes, rule bodies, FUNCTIONs, RULEs and CONSTANTs are skipped. Anything else the parser does
not recognise stops it with an error, so nothing is dropped silently.
"""

import hashlib
import re
import sys

EDITIONS = [
    # (const name, FILE_SCHEMA string, source URL, label)
    (
        "AP242_ED1",
        "AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF { 1 0 10303 442 1 1 4 }",
        "https://raw.githubusercontent.com/stepcode/stepcode/"
        "74b6fe45751bd60be749bc80766f38745d29ed72/data/ap242/242_n8324_mim_lf.exp",
        "AP242 edition 1 (ISO 10303-242:2014), WG12 N8324",
    ),
    (
        "AP242_ED4",
        "AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF { 1 0 10303 442 7 1 4 }",
        "https://raw.githubusercontent.com/stepcode/stepcode/"
        "8cc8fc75905a10333e5bf29b6e6a568d737dffa3/data/ap242/242_mim_lf.exp",
        "AP242 edition 4 (ISO 10303-242:2025), WG12 N11521",
    ),
]

SIMPLE = {"INTEGER", "REAL", "NUMBER", "STRING", "BOOLEAN", "LOGICAL", "BINARY"}
AGGR = {"LIST", "SET", "BAG", "ARRAY"}
SECTIONS = {"DERIVE", "INVERSE", "UNIQUE", "WHERE", "END_ENTITY"}


def tokenize(text):
    """EXPRESS tokens: words (upper-cased keywords keep their case for names), numbers, strings and
    punctuation; (* nested *) and -- tail remarks removed."""
    out = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c.isspace():
            i += 1
        elif text.startswith("(*", i):
            depth, i = 1, i + 2
            while depth:
                if i >= n:
                    raise SystemExit("unterminated remark")
                if text.startswith("(*", i):
                    depth, i = depth + 1, i + 2
                elif text.startswith("*)", i):
                    depth, i = depth - 1, i + 2
                else:
                    i += 1
        elif text.startswith("--", i):
            j = text.find("\n", i)
            i = n if j < 0 else j
        elif c == "'":
            j = i + 1
            while True:
                k = text.find("'", j)
                if k < 0:
                    raise SystemExit("unterminated string")
                if text.startswith("''", k):
                    j = k + 2
                else:
                    break
            out.append(text[i : k + 1])
            i = k + 1
        elif c.isalpha() or c == "_":
            m = re.compile(r"[A-Za-z_][A-Za-z0-9_]*").match(text, i)
            out.append(m.group(0))
            i = m.end()
        elif c.isdigit():
            m = re.compile(r"\d+(\.\d*)?([eE][+-]?\d+)?").match(text, i)
            out.append(m.group(0))
            i = m.end()
        else:
            for p in (":=:", ":<>:", ":=", "<*", "<=", ">=", "<>", "||", "**"):
                if text.startswith(p, i):
                    out.append(p)
                    i += len(p)
                    break
            else:
                out.append(c)
                i += 1
    return out


class Parser:
    def __init__(self, toks):
        self.t = toks
        self.i = 0

    def peek(self, k=0):
        return self.t[self.i + k] if self.i + k < len(self.t) else None

    def kw(self, k=0):
        p = self.peek(k)
        return p.upper() if p is not None else None

    def next(self):
        tok = self.t[self.i]
        self.i += 1
        return tok

    def expect(self, want):
        got = self.next()
        if got.upper() != want:
            raise SystemExit(f"expected {want!r}, got {got!r} near token {self.i}: {self.t[self.i - 8 : self.i + 8]}")
        return got

    def skip_to(self, *stops):
        """Skip balanced tokens up to (not including) one of `stops` at depth 0."""
        depth = 0
        while True:
            tok = self.peek()
            if tok is None:
                raise SystemExit(f"end of file looking for {stops}")
            if depth == 0 and tok.upper() in stops:
                return
            if tok in ("(", "["):
                depth += 1
            elif tok in (")", "]"):
                depth -= 1
            self.i += 1

    # -- types ---------------------------------------------------------------------------------

    def bound(self):
        """A bound expression up to ':' or ']': a literal integer, '?', or None (not literal)."""
        start = self.i
        self.skip_to(":", "]")
        toks = self.t[start : self.i]
        if len(toks) == 1 and toks[0].isdigit():
            return int(toks[0])
        if toks == ["?"]:
            return "?"
        return None

    def type_ref(self):
        k = self.kw()
        if k in AGGR:
            self.next()
            lo, hi = 0, "?"
            if self.peek() == "[":
                self.next()
                lo = self.bound()
                self.expect(":")
                hi = self.bound()
                self.expect("]")
            elif k == "ARRAY":
                raise SystemExit("ARRAY without bounds")
            self.expect("OF")
            optional = unique = False
            while self.kw() in ("OPTIONAL", "UNIQUE"):
                if self.next().upper() == "OPTIONAL":
                    optional = True
                else:
                    unique = True
            elem = self.type_ref()
            return ("aggr", k, lo, hi, optional, unique, elem)
        if k in SIMPLE:
            self.next()
            if self.peek() == "(":  # width or precision: not checked
                while self.next() != ")":
                    pass
                if self.kw() == "FIXED":
                    self.next()
            return ("simple", k)
        if k == "GENERIC_ENTITY" or k == "GENERIC" or k == "AGGREGATE":
            raise SystemExit(f"unsupported type {k}")
        name = self.next()
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name):
            raise SystemExit(f"bad type name {name!r}")
        return ("named", name.lower())

    def name_list(self):
        self.expect("(")
        names = []
        while True:
            names.append(self.next().lower())
            sep = self.next()
            if sep == ")":
                return names
            if sep != ",":
                raise SystemExit(f"bad list separator {sep!r}")

    def type_decl(self):
        self.expect("TYPE")
        name = self.next().lower()
        self.expect("=")
        if self.kw() == "EXTENSIBLE" or self.kw() == "GENERIC_ENTITY" or self.kw() == "BASED_ON":
            raise SystemExit(f"TYPE {name}: extensible or based-on types are not supported")
        if self.kw() == "SELECT":
            self.next()
            body = ("select", sorted(set(self.name_list())))
        elif self.kw() == "ENUMERATION":
            self.next()
            self.expect("OF")
            body = ("enum", self.name_list())
        else:
            body = ("defined", self.type_ref())
        self.expect(";")
        rules = []
        if self.kw() == "WHERE":
            self.next()
            rules = self.rules("END_TYPE")
        self.expect("END_TYPE")
        self.expect(";")
        return name, body, rules

    def rules(self, end, unique=False):
        """Labels of the domain or uniqueness rules up to `end` (the next section keyword)."""
        labels = []
        while self.kw() not in end:
            if self.peek(1) == ":":
                labels.append(self.next().upper())
                self.next()
            self.skip_to(";")
            self.expect(";")
        return labels

    # -- entities ------------------------------------------------------------------------------

    def supertype_expr(self):
        """SUPERTYPE OF ( expr ) with AND binding tighter than ANDOR."""
        def primary():
            if self.kw() == "ONEOF":
                self.next()
                self.expect("(")
                ops = [andor()]
                while self.peek() == ",":
                    self.next()
                    ops.append(andor())
                self.expect(")")
                return ("ONEOF", ops)
            if self.peek() == "(":
                self.next()
                e = andor()
                self.expect(")")
                return e
            return ("ENTITY", self.next().lower())

        def and_():
            ops = [primary()]
            while self.kw() == "AND":
                self.next()
                ops.append(primary())
            return ops[0] if len(ops) == 1 else ("AND", ops)

        def andor():
            ops = [and_()]
            while self.kw() == "ANDOR":
                self.next()
                ops.append(and_())
            return ops[0] if len(ops) == 1 else ("ANDOR", ops)

        self.expect("(")
        e = andor()
        self.expect(")")
        return e

    def attr_name(self):
        """`a` or `SELF\\e.a` (redeclaration), with an optional RENAMED alias."""
        if self.kw() == "SELF":
            self.next()
            self.expect("\\")
            owner = self.next().lower()
            self.expect(".")
            attr = self.next().lower()
            if self.kw() == "RENAMED":
                self.next()
                self.next()
            return (owner, attr)
        return (None, self.next().lower())

    def entity_decl(self):
        self.expect("ENTITY")
        name = self.next().lower()
        abstract = False
        expr = None
        supers = []
        while self.peek() != ";" and self.kw() not in SECTIONS and not self.is_attr_start():
            k = self.kw()
            if k == "ABSTRACT":
                self.next()
                abstract = True
                if self.kw() == "SUPERTYPE":
                    self.next()
                    if self.kw() == "OF":
                        self.next()
                        expr = self.supertype_expr()
            elif k == "SUPERTYPE":
                self.next()
                self.expect("OF")
                expr = self.supertype_expr()
            elif k == "SUBTYPE":
                self.next()
                self.expect("OF")
                supers = self.name_list()
            else:
                raise SystemExit(f"ENTITY {name}: unexpected {self.peek()!r}")
        if self.peek() == ";":
            self.next()
        attrs, redecls, rules = [], [], []
        # explicit attributes
        while self.kw() not in SECTIONS:
            names = [self.attr_name()]
            while self.peek() == ",":
                self.next()
                names.append(self.attr_name())
            self.expect(":")
            optional = False
            if self.kw() == "OPTIONAL":
                self.next()
                optional = True
            ty = self.type_ref()
            self.expect(";")
            for owner, attr in names:
                if owner is None:
                    attrs.append((attr, optional, ty))
                else:
                    redecls.append((owner, attr, False, optional, ty))
        while self.kw() != "END_ENTITY":
            sect = self.next().upper()
            if sect == "DERIVE":
                while self.kw() not in SECTIONS:
                    owner, attr = self.attr_name()
                    self.expect(":")
                    ty = self.type_ref()
                    self.expect(":=")
                    self.skip_to(";")
                    self.expect(";")
                    if owner is not None:
                        redecls.append((owner, attr, True, False, ty))
            elif sect == "INVERSE":
                while self.kw() not in SECTIONS:
                    self.skip_to(";")
                    self.expect(";")
            elif sect in ("UNIQUE", "WHERE"):
                rules += self.rules(SECTIONS)
            else:
                raise SystemExit(f"ENTITY {name}: unexpected section {sect}")
        self.expect("END_ENTITY")
        self.expect(";")
        return name, abstract, expr, supers, attrs, redecls, rules

    def is_attr_start(self):
        return self.kw() == "SELF" or (self.peek(1) in (":", ","))

    def schema(self):
        types, entities = {}, {}
        while self.peek() is not None:
            k = self.kw()
            if k == "TYPE":
                n, body, rules = self.type_decl()
                types[n] = (body, rules)
            elif k == "ENTITY":
                e = self.entity_decl()
                entities[e[0]] = e
            elif k in ("FUNCTION", "PROCEDURE", "RULE"):
                self.skip_block(k)
            elif k == "CONSTANT":
                while self.kw() != "END_CONSTANT":
                    self.next()
                self.next()
                self.expect(";")
            elif k in ("SCHEMA",):
                self.next()
                self.next()
                if self.peek() == "'":
                    self.next()
                self.skip_to(";")
                self.expect(";")
            elif k == "END_SCHEMA":
                self.next()
                self.expect(";")
            elif k == "SUBTYPE_CONSTRAINT":
                raise SystemExit("SUBTYPE_CONSTRAINT is not supported")
            else:
                raise SystemExit(f"unexpected top-level token {self.peek()!r}")
        return types, entities

    def skip_block(self, k):
        depth = 0
        while True:
            tok = self.kw()
            self.next()
            if tok in ("FUNCTION", "PROCEDURE", "RULE"):
                depth += 1
            elif tok in ("END_FUNCTION", "END_PROCEDURE", "END_RULE"):
                depth -= 1
                if depth == 0:
                    self.expect(";")
                    return


def check(types, entities):
    """Every name a declaration uses must be declared."""
    def ty_names(ty):
        if ty[0] == "named":
            yield ty[1]
        elif ty[0] == "aggr":
            yield from ty_names(ty[6])

    def expr_names(e):
        if e[0] == "ENTITY":
            yield e[1]
        else:
            for op in e[1]:
                yield from expr_names(op)

    known = set(types) | set(entities)
    for n, (body, _) in types.items():
        used = body[1] if body[0] == "select" else (list(ty_names(body[1])) if body[0] == "defined" else [])
        for u in used:
            if u not in known:
                raise SystemExit(f"TYPE {n} uses undeclared {u}")
    for n, (_, _, expr, supers, attrs, redecls, _) in entities.items():
        for s in supers:
            if s not in entities:
                raise SystemExit(f"ENTITY {n}: undeclared supertype {s}")
        if expr:
            for s in expr_names(expr):
                if s not in entities or n not in entities[s][3]:
                    raise SystemExit(f"ENTITY {n}: SUPERTYPE OF names {s}, not a subtype")
        for a in attrs:
            for u in ty_names(a[2]):
                if u not in known:
                    raise SystemExit(f"ENTITY {n}.{a[0]} uses undeclared {u}")
        for r in redecls:
            if r[0] not in entities:
                raise SystemExit(f"ENTITY {n}: redeclares {r[0]}.{r[1]}, undeclared entity")


def declaring(entities, owner, attr):
    """The entity among `owner` and its supertypes that declares explicit attribute `attr`:
    `SELF\\e.a` may name a supertype that itself inherited (or redeclared) `a`."""
    seen, stack = set(), [owner]
    found = set()
    while stack:
        e = stack.pop()
        if e in seen:
            continue
        seen.add(e)
        if any(a[0] == attr for a in entities[e][4]):
            found.add(e)
        stack.extend(entities[e][3])
    if len(found) != 1:
        raise SystemExit(f"{owner}.{attr}: declared by {sorted(found)}, expected exactly one")
    return found.pop()


def resolve_redeclarations(entities):
    """Name every redeclared attribute by the entity that declares it explicitly."""
    for n, e in entities.items():
        redecls = [(declaring(entities, r[0], r[1]),) + r[1:] for r in e[5]]
        entities[n] = e[:5] + (redecls,) + e[6:]


def rs_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def rs_strs(xs):
    return "&[" + ", ".join(rs_str(x) for x in xs) + "]"


def rs_ty(ty):
    if ty[0] == "simple":
        return "Ty::" + {
            "INTEGER": "Integer",
            "REAL": "Real",
            "NUMBER": "Number",
            "STRING": "String",
            "BOOLEAN": "Boolean",
            "LOGICAL": "Logical",
            "BINARY": "Binary",
        }[ty[1]]
    if ty[0] == "named":
        return f"Ty::Named({rs_str(ty[1])})"
    _, kind, lo, hi, optional, unique, elem = ty
    kind = {"LIST": "List", "SET": "Set", "BAG": "Bag", "ARRAY": "Array"}[kind]

    def b(x):
        return "None" if x in (None, "?") else f"Some({x})"

    return (
        f"Ty::Aggregate(&Aggregate {{ kind: AggregateKind::{kind}, lower: {b(lo)}, upper: {b(hi)}, "
        f"optional: {str(optional).lower()}, unique: {str(unique).lower()}, of: {rs_ty(elem)} }})"
    )


def rs_expr(e):
    if e[0] == "ENTITY":
        return f"Sx::Entity({rs_str(e[1])})"
    kind = {"ONEOF": "OneOf", "ANDOR": "AndOr", "AND": "And"}[e[0]]
    return f"Sx::{kind}(&[" + ", ".join(rs_expr(op) for op in e[1]) + "])"


def emit_edition(out, const, file_schema, url, label, sha, types, entities):
    out.append(f"/// {label}.")
    out.append("///")
    out.append(f"/// Source: <{url}>")
    out.append(f"/// (sha256 `{sha}`).")
    out.append(f"pub static {const}: Edition = Edition {{")
    out.append(f"    file_schema: {rs_str(file_schema)},")
    out.append(f"    source_url: {rs_str(url)},")
    out.append(f"    source_sha256: {rs_str(sha)},")
    out.append("    types: &[")
    for n in sorted(types):
        body, rules = types[n]
        if body[0] == "select":
            d = f"TypeDef::Select({rs_strs(body[1])})"
        elif body[0] == "enum":
            d = f"TypeDef::Enumeration({rs_strs([v.lower() for v in body[1]])})"
        else:
            d = f"TypeDef::Defined({rs_ty(body[1])})"
        out.append(f"        TypeDecl {{ name: {rs_str(n)}, def: {d}, rules: {rs_strs(rules)} }},")
    out.append("    ],")
    out.append("    entities: &[")
    for n in sorted(entities):
        _, abstract, expr, supers, attrs, redecls, rules = entities[n]
        out.append("        EntityDecl {")
        out.append(f"            name: {rs_str(n)},")
        out.append(f"            is_abstract: {str(abstract).lower()},")
        out.append(f"            supertypes: {rs_strs(supers)},")
        out.append(f"            supertype_of: {'None' if expr is None else 'Some(' + rs_expr(expr) + ')'},")
        if attrs:
            out.append("            attributes: &[")
            for a, opt, ty in attrs:
                out.append(f"                Attribute {{ name: {rs_str(a)}, optional: {str(opt).lower()}, ty: {rs_ty(ty)} }},")
            out.append("            ],")
        else:
            out.append("            attributes: &[],")
        if redecls:
            out.append("            redeclared: &[")
            for owner, a, derived, opt, ty in redecls:
                out.append(
                    f"                Redeclared {{ entity: {rs_str(owner)}, attribute: {rs_str(a)}, "
                    f"derived: {str(derived).lower()}, optional: {str(opt).lower()}, ty: {rs_ty(ty)} }},"
                )
            out.append("            ],")
        else:
            out.append("            redeclared: &[],")
        out.append(f"            rules: {rs_strs(rules)},")
        out.append("        },")
    out.append("    ],")
    out.append("};")


def main(argv):
    if len(argv) != len(EDITIONS):
        raise SystemExit(__doc__)
    out = [
        "// @generated by tools/express_table.py: do not edit.",
        "//",
        "// python3 -I tools/express_table.py 242_n8324_mim_lf.exp 242_mim_lf.exp \\",
        "//     > crates/haecceity/src/express_table.rs",
        "//",
    ]
    body = []
    for (const, file_schema, url, label), path in zip(EDITIONS, argv):
        data = open(path, "rb").read()
        sha = hashlib.sha256(data).hexdigest()
        types, entities = Parser(tokenize(data.decode("utf-8"))).schema()
        check(types, entities)
        resolve_redeclarations(entities)
        out.append(f"// {const}: {url}")
        out.append(f"//     sha256 {sha}; {len(types)} types, {len(entities)} entities")
        body.append("")
        emit_edition(body, const, file_schema, url, label, sha, types, entities)
    out.append("")
    out.append("use crate::express::{")
    out.append("    Aggregate, AggregateKind, Attribute, Edition, EntityDecl, Redeclared, Sx, Ty, TypeDecl, TypeDef,")
    out.append("};")
    sys.stdout.write("\n".join(out + body) + "\n")


if __name__ == "__main__":
    main(sys.argv[1:])
