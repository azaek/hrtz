// How the new features are found: being decided in the features lab (`lab-fun.html`), each
// direction a way to surface the blind test, Your week and the games card. Every direction has the
// "New" badges; they differ in what else invites you in. Once picked, the others go.
//
// - badges:  each feature where it belongs, with its "New" badge until seen, and nothing else.
// - front:   "New in hrtz" at the top of the Sensor page: the three, each a button away, until
//            hidden.
// - moments: each at the moment it matters: the blind test from the polling rate card, the games
//            card under the DPI stages, and Your week as a pop-up once a week with something in it.

export type Discovery = "badges" | "front" | "moments";

let mode: Discovery = "moments";

export const discovery = () => mode;

export function setDiscovery(next: Discovery) {
  mode = next;
}

// The lab shows Your week's pop-up whenever its page opens, not once a week.
let always = false;
export const weekMomentAlways = () => always;
export function setWeekMomentAlways(on: boolean) {
  always = on;
}
