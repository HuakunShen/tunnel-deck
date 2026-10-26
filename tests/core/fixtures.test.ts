/**
 * Golden fixtures adjudicating the TypeScript and Rust implementations.
 *
 * `fixtures/argv/*.json` pins the exact ssh argv both builders must produce;
 * `fixtures/state-machine.json` pins every allowed and rejected transition.
 * A change to either domain must update the fixture first, so the other
 * language fails loudly instead of drifting silently.
 */
import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { buildSshArgv, canTransition } from "@ssh-tunnel/core";
import { tunnelProfileSchema, type TunnelStatus } from "@ssh-tunnel/contract";

const FIXTURES = join(__dirname, "..", "..", "fixtures");

interface ArgvFixture {
  readonly name: string;
  readonly profile: unknown;
  readonly options: { readonly controlPath?: string; readonly interactive?: boolean };
  readonly mustContain: readonly string[];
  readonly mustNotContain: readonly string[];
}

describe("golden argv fixtures", () => {
  const dir = join(FIXTURES, "argv");
  const files = readdirSync(dir).filter((file) => file.endsWith(".json")).sort();
  expect(files.length).toBeGreaterThan(0);
  for (const file of files) {
    it(file, () => {
      const fixture = JSON.parse(readFileSync(join(dir, file), "utf8")) as ArgvFixture;
      const profile = tunnelProfileSchema.parse(fixture.profile);
      const { argv } = buildSshArgv(profile, fixture.options);
      const joined = argv.join(" ");
      for (const token of fixture.mustContain) {
        expect(joined, `${fixture.name}: expected ${token}`).toContain(token);
      }
      for (const token of fixture.mustNotContain) {
        expect(joined, `${fixture.name}: unexpected ${token}`).not.toContain(token);
      }
    });
  }
});

describe("golden state-machine fixtures", () => {
  it("accepts and rejects exactly the pinned transitions", () => {
    const raw = JSON.parse(readFileSync(join(FIXTURES, "state-machine.json"), "utf8")) as {
      transitions: readonly { from: TunnelStatus; to: TunnelStatus; allowed: boolean }[];
    };
    expect(raw.transitions.length).toBeGreaterThan(0);
    for (const { from, to, allowed } of raw.transitions) {
      expect(canTransition(from, to), `${from} -> ${to}`).toBe(allowed);
    }
  });
});
