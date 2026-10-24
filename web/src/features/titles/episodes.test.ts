import { describe, expect, test } from "vitest";
import type { Season } from "@/api/schemas";
import {
  episodeKey,
  markThroughPlays,
  nextEpisode,
  orderSeasons,
  throughEpisodes,
} from "./episodes";

const SEASONS: Season[] = [{ number: 2 }, { number: 0 }, { number: 1 }];
const EPISODES: Record<number, number[]> = {
  0: [1, 2],
  1: [1, 2, 3],
  2: [1, 2],
};
const episodesOf = async (season: number) => EPISODES[season] ?? [];

describe("episodes", () => {
  test("specials come last", () => {
    expect(orderSeasons(SEASONS).map((s) => s.number)).toEqual([1, 2, 0]);
  });

  test.each([
    [[], { season: 1, episode: 1 }],
    [[{ season: 1, episode: 1 }], { season: 1, episode: 2 }],
    [[{ season: 1, episode: 3 }], { season: 2, episode: 1 }],
    // A gap before the furthest episode is not "next"; specials never are.
    [
      [
        { season: 1, episode: 2 },
        { season: 0, episode: 2 },
      ],
      { season: 1, episode: 3 },
    ],
    [[{ season: 2, episode: 2 }], null],
  ])("after %j the next episode is %j", async (watched, expected) => {
    expect(await nextEpisode(SEASONS, watched, episodesOf)).toEqual(expected);
  });

  test("only the seasons the answer needs are read", async () => {
    const asked: number[] = [];
    await nextEpisode(SEASONS, [{ season: 2, episode: 1 }], async (s) => {
      asked.push(s);
      return EPISODES[s] ?? [];
    });
    expect(asked).toEqual([2]);
  });

  test("marking up to a special lists the unwatched specials before it", () => {
    const specials = [1, 2, 3].map((number) => ({ season: 0, number }));
    expect(
      markThroughPlays(
        { season: 0, episode: 3 },
        specials,
        new Set([episodeKey(0, 2)]),
      ),
    ).toEqual({
      kind: "episodes",
      episodes: [
        { season: 0, episode: 1 },
        { season: 0, episode: 3 },
      ],
    });
    expect(
      markThroughPlays({ season: 0, episode: 1 }, specials, new Set(["0:1"])),
    ).toBeNull();
  });

  test("through covers the loaded episodes up to the target", () => {
    const loaded = (s: number) =>
      (EPISODES[s] ?? []).map((number) => ({ season: s, number }));
    expect(throughEpisodes({ season: 2, episode: 1 }, SEASONS, loaded)).toEqual(
      [
        { season: 1, episode: 1 },
        { season: 1, episode: 2 },
        { season: 1, episode: 3 },
        { season: 2, episode: 1 },
      ],
    );
  });
});
