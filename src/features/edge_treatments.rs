//! Structural edge treatments used to measure their supporting features (#134).
//! Reuse the treatment proofs; a merely sloping or curved neighbour is not an edge break.

use super::evidence::Occurrence;
use super::{Context, chamfers, fillets, oriented_chamfers};

pub(super) fn discover(ctx: &Context<'_>) -> Vec<Occurrence<()>> {
    let erase = |defining, context| Occurrence {
        record: (),
        defining,
        context,
    };
    let mut out: Vec<_> = chamfers::discover(ctx, &Default::default())
        .into_iter()
        .map(|o| erase(o.defining, o.context))
        .chain(
            oriented_chamfers::discover(ctx, &Default::default())
                .into_iter()
                .map(|o| erase(o.defining, o.context)),
        )
        .collect();
    // Reporting's absolute 0.6 mm radius floor does not make a smaller real round
    // cease trimming its supporting face.
    out.extend(
        fillets::discover(
            ctx,
            &fillets::FilletOptions {
                min_radius: Some(0.0),
                ..Default::default()
            },
        )
        .into_iter()
        .filter(|o| o.record.side == "convex")
        .map(|o| erase(o.defining, o.context)),
    );
    out
}
