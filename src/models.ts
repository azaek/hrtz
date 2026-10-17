// Generated from the vendor app's model list and renders (credited in assets/models/NOTICE.md).
// Hotspot centres are in image pixels; keys are flash button indices.

import mid1 from "./assets/models/mid1.png";
import mid2 from "./assets/models/mid2.png";
import mid3 from "./assets/models/mid3.png";
import mid4 from "./assets/models/mid4.png";
import mid5 from "./assets/models/mid5.png";
import mid6 from "./assets/models/mid6.png";

export interface ModelArt {
  name: string;
  image: string;
  size: [number, number];
  hotspots: Record<number, [number, number]>;
}

export const MODEL_ART: Record<number, ModelArt> = {
  1: { name: "ZI PRO", image: mid1, size: [298, 263], hotspots: {"0": [105, 203], "1": [34, 163], "2": [91, 134], "4": [216, 166], "3": [253, 131], "5": [180, 128], "13": [290, 236], "14": [246, 236], "15": [202, 236]} },
  2: { name: "Z2", image: mid2, size: [310, 276], hotspots: {"0": [99, 218], "1": [34, 183], "2": [86, 157], "4": [208, 165], "3": [240, 131], "5": [213, 210], "13": [294, 237], "14": [250, 237], "15": [206, 237]} },
  3: { name: "Z2 mini", image: mid3, size: [308, 293], hotspots: {"0": [96, 240], "1": [32, 196], "2": [88, 177], "4": [205, 186], "3": [231, 141], "5": [167, 167], "13": [291, 251], "14": [247, 251], "15": [203, 251]} },
  4: { name: "Z1 PRO MAX", image: mid4, size: [298, 263], hotspots: {"0": [105, 203], "1": [34, 163], "2": [91, 134], "4": [216, 166], "3": [253, 131], "5": [180, 128], "13": [290, 236], "14": [246, 236], "15": [202, 236]} },
  5: { name: "Z2 MAX", image: mid5, size: [310, 276], hotspots: {"0": [99, 218], "1": [34, 183], "2": [86, 157], "4": [208, 165], "3": [240, 131], "5": [213, 210], "13": [294, 237], "14": [250, 237], "15": [206, 237]} },
  6: { name: "Z2 mini MAX", image: mid6, size: [308, 293], hotspots: {"0": [96, 240], "1": [32, 196], "2": [88, 177], "4": [205, 186], "3": [231, 141], "5": [167, 167], "13": [291, 251], "14": [247, 251], "15": [203, 251]} },
};
