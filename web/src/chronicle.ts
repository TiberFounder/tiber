// The Chronicle: the twins' story after the oath, told one chapter at a time. Each city's
// chapters open as its curve raises SOL; the last opens when both cities have graduated.

export type Chapter = {
  id: string;
  title: string;
  /** What a locked chapter shows. */
  teaser: string;
  paragraphs: string[];
};

/** SOL raised on a city's curve that opens each of its chapters; the last is graduation. */
export const MILESTONES_SOL = [5, 10, 20, 30] as const;

export const CHAPTERS: Record<"rom" | "rem", Chapter[]> = {
  rom: [
    {
      id: "rom-1",
      title: "I. The Furrow",
      teaser: "Romulus takes up a plough on the Palatine.",
      paragraphs: [
        "Romulus yoked a white bull and a white cow to a bronze plough and cut a furrow around the Palatine. Where the furrow ran, the wall would rise. Where he lifted the plough, a gate would stand.",
        "He left one gate wider than the rest, facing the river, facing his brother's hill. \"So the tithe can pass,\" he said, and no one on the Palatine argued.",
      ],
    },
    {
      id: "rom-2",
      title: "II. The Open Grove",
      teaser: "Strangers arrive at the Palatine gate.",
      paragraphs: [
        "A city of shepherds is a small city. So Romulus opened a grove between the two peaks of his hill and let it be known that anyone who came there could stay: runaways, debtors, wanderers, the ones no other city wanted.",
        "They came with nothing and were given a place at the wall. Each one who bought into the city paid the tithe across the river, and each one who left gave an offering to the flame. The Palatine grew, and Remus's hill grew with it.",
      ],
    },
    {
      id: "rom-3",
      title: "III. Twelve Birds",
      teaser: "The vultures return over the Palatine.",
      paragraphs: [
        "On the anniversary of the augury, twelve vultures circled the Palatine again. In the old telling they were the reason for the quarrel. This time Romulus counted them and laughed.",
        "\"Twelve for me, six for my brother,\" he said, \"and every bird that lands on my hill sends a feather to his.\" He sent a runner to the Aventine with the count, and a gift of grain.",
      ],
    },
    {
      id: "rom-4",
      title: "IV. The Wall Stands",
      teaser: "The Palatine wall is finished.",
      paragraphs: [
        "The last stone was set in the Palatine wall, and the curve of the city closed. What had been gathered was sealed into the river's keeping, where no hand, not even a founder's, could take it back out.",
        "Romulus walked the full circuit of the wall at dusk. At the river gate he stopped, looked across to the Aventine, and waited for his brother's wall to rise to meet it.",
      ],
    },
  ],
  rem: [
    {
      id: "rem-1",
      title: "I. Six Birds",
      teaser: "Remus keeps watch on the Aventine.",
      paragraphs: [
        "Remus had seen his six vultures first. Everyone on the Aventine knew it, and they never let the Palatine forget it. First is not the same as most, but it is not nothing.",
        "He built no gate at first. He built a fire, on the highest rock of the Aventine, so his brother could see it from across the river: a promise that the oath was kept on this hill too.",
      ],
    },
    {
      id: "rem-2",
      title: "II. The River Gate",
      teaser: "A gate opens toward the Tiber.",
      paragraphs: [
        "When the first tithe came across the river from the Palatine, Remus did not keep it. He carried it to Vesta's flame and burned it, as the oath required, so his own people's share grew dearer.",
        "Then he cut a gate in the Aventine facing the Palatine's wide gate, so the river path ran straight between them. Traders began to call it the Twins' Road.",
      ],
    },
    {
      id: "rem-3",
      title: "III. The Leap",
      teaser: "Remus jumps a wall, as he did in the old story.",
      paragraphs: [
        "In the old telling, Remus mocked his brother's low wall by leaping over it, and died for it. In this telling, Remus leapt the Palatine wall with a sack over his shoulder.",
        "Inside it was the Aventine's tithe for the season, every coin of it. He set it at his brother's feet, and Romulus carried it to the flame himself. The guards who had drawn their spears put them away and never drew them at a twin again.",
      ],
    },
    {
      id: "rem-4",
      title: "IV. The Aventine Wall",
      teaser: "The Aventine wall is finished.",
      paragraphs: [
        "The Aventine wall closed its circle, and what the city had raised was given to the river's keeping, beyond any hand's reach.",
        "That night the fire on the Aventine rock burned higher than it ever had, and across the water the Palatine answered with its own.",
      ],
    },
  ],
};

/** Opens when both cities have graduated. */
export const FINALE: Chapter = {
  id: "finale",
  title: "V. One Wolf",
  teaser: "When both walls stand, the wolf returns.",
  paragraphs: [
    "With both walls standing, the she-wolf came down to the river one last time. She drank, looked up at the two hills and their two fires, and lay down on the Twins' Road between the gates.",
    "Two cities, she had said, that feed each other. They had. Neither would ever take the other's hill, because neither needed to: every buy on one hill burned a little of the other, and every coin that left either one went to the flame.",
    "Some say the city that grew from the two hills was named for one brother. In this telling, no one could ever agree which.",
  ],
};
