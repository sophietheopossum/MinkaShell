.pragma library

// Pure layout math for the dock's drag-to-reorder: no Qt or Quickshell, so
// it unit-tests offscreen. `ids` are window ids in chip order, `groupOf`
// maps id -> workspace group key, `slots[i]` is {x, w}: chip i's Row
// position and width, which the Translate offsets never change.

// Inclusive index range of the contiguous group around ids[i].
function groupSpan(ids, groupOf, i) {
    const key = groupOf[ids[i]];
    let start = i;
    let end = i;
    while (start > 0 && groupOf[ids[start - 1]] === key)
        start--;
    while (end < ids.length - 1 && groupOf[ids[end + 1]] === key)
        end++;
    return { start: start, end: end };
}

// Left edge of a chip of width `w` moved from index `from` to `to`. Exact
// for a Row with uniform spacing: every chip it passes shifts by w + spacing.
function landingX(slots, from, to, w) {
    return to > from ? slots[to].x + slots[to].w - w : slots[to].x;
}

// Live drag. The chip follows the pointer, clamped to its group and, when
// that leaves room, to the visible viewport [viewLo, viewHi]. The plan also
// carries the group's extent [groupLo, groupEnd], which bounds edge
// auto-scroll so the strip never scrolls the dragged chip out of view. It passes a
// neighbour once its centre crosses the midpoint between that neighbour's
// centre undisplaced and displaced. Thresholds come from Row slots only, so
// the displacement never feeds back into the target (no oscillation), and
// both group ends stay reachable at the clamp.
function livePlan(ids, groupOf, slots, spacing, dragId, pointerX, grabOffset, viewLo, viewHi, cancelled) {
    const from = ids.indexOf(dragId);
    if (from < 0 || !slots[from])
        return null;
    const span = groupSpan(ids, groupOf, from);
    const w = slots[from].w;
    const groupLo = slots[span.start].x;
    const groupHi = Math.max(groupLo, slots[span.end].x + slots[span.end].w - w);
    let lo = Math.max(groupLo, viewLo);
    let hi = Math.min(groupHi, viewHi - w);
    if (lo > hi) {
        lo = groupLo;
        hi = groupHi;
    }
    const left = Math.max(lo, Math.min(hi, pointerX - grabOffset));
    let to = from;
    if (!cancelled) {
        const centre = left + w / 2;
        const half = (w + spacing) / 2;
        for (let i = from + 1; i <= span.end && centre > slots[i].x + slots[i].w / 2 - half; i++)
            to = i;
        if (to === from) {
            for (let i = from - 1; i >= span.start && centre < slots[i].x + slots[i].w / 2 + half; i--)
                to = i;
        }
    }
    return { from: from, to: to, left: left, width: w, landingX: landingX(slots, from, to, w),
             groupLo: groupLo, groupEnd: slots[span.end].x + slots[span.end].w };
}

// A drop still waiting on ShojiWM: the chip sits in the slot `move` gives
// it. null when the move no longer resolves (window or anchor gone, or the
// anchor outside the group).
function pendingPlan(ids, groupOf, slots, move) {
    const from = ids.indexOf(move.id);
    if (from < 0 || !slots[from])
        return null;
    const span = groupSpan(ids, groupOf, from);
    let to = span.end;
    if (move.beforeId !== null) {
        const b = ids.indexOf(move.beforeId);
        if (b < span.start || b > span.end)
            return null;
        to = b > from ? b - 1 : b;
    }
    const w = slots[from].w;
    const x = landingX(slots, from, to, w);
    return { from: from, to: to, left: x, width: w, landingX: x };
}

// Window id -> Translate x for a plan.
function shiftsFor(ids, slots, spacing, plan) {
    const shifts = {};
    if (!plan)
        return shifts;
    const gap = plan.width + spacing;
    shifts[ids[plan.from]] = plan.left - slots[plan.from].x;
    for (let i = plan.from + 1; i <= plan.to; i++)
        shifts[ids[i]] = -gap;
    for (let i = plan.to; i < plan.from; i++)
        shifts[ids[i]] = gap;
    return shifts;
}

// windows.reorder anchor for moving ids[from] to `to` (to !== from): the
// window it must end up directly before, or null for the end of its group,
// which is also the end of its workspace.
function beforeIdFor(ids, groupOf, from, to) {
    if (to < from)
        return ids[to];
    const span = groupSpan(ids, groupOf, from);
    return to < span.end ? ids[to + 1] : null;
}

// True once the live order shows `move`, or once it can no longer apply
// (a window gone, or the two now in different groups).
function moveSatisfied(ids, groupOf, move) {
    const i = ids.indexOf(move.id);
    if (i < 0)
        return true;
    if (move.beforeId === null)
        return i === ids.length - 1 || groupOf[ids[i + 1]] !== groupOf[move.id];
    const b = ids.indexOf(move.beforeId);
    if (b < 0 || groupOf[move.beforeId] !== groupOf[move.id])
        return true;
    return b === i + 1;
}

// A held order is still showable: the same windows, each in the same group.
function sameMembers(heldIds, heldGroups, liveIds, groupOf) {
    if (heldIds.length !== liveIds.length)
        return false;
    for (let k = 0; k < heldIds.length; k++) {
        if (groupOf[heldIds[k]] !== heldGroups[k])
            return false;
    }
    return true;
}
