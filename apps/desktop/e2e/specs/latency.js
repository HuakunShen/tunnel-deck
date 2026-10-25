/**
 * Row-action feedback latency, measured in the *real* app.
 *
 * The bug this pins: Tauri runs a synchronous `#[tauri::command]` inline on
 * the IPC caller's thread — the main thread, which is also the thread that
 * paints the WebView. `tunnel_start`/`tunnel_stop` block for the ssh grace
 * window, so a sync command kept "Starting…" off screen until it returned and
 * the click looked frozen. Commands are async + `spawn_blocking` now; this
 * spec fails if that regresses.
 *
 * The measurement is deliberately UI-side: click, then poll the DOM for the
 * optimistic label. If the main thread is blocked, the label cannot appear.
 */
describe("row action feedback", () => {
  it("paints Starting… immediately, not after the command returns", async () => {
    const row = await browser.$(".tunnel-row");
    await row.waitForExist({ timeout: 30000 });

    const readRow = () =>
      browser.execute(() => {
        const element = document.querySelector(".tunnel-row");
        if (element === null) return null;
        const button = [...element.querySelectorAll("button")].find((candidate) =>
          /^(Start|Stop|Starting…|Stopping…)$/.test(candidate.textContent.trim()),
        );
        const status = [...element.querySelectorAll("span")].find((candidate) =>
          /^(starting|stopping|running|degraded|failed|stopped)$/i.test(
            candidate.textContent.trim(),
          ),
        );
        return {
          button: button === undefined ? null : button.textContent.trim(),
          disabled: button === undefined ? null : button.disabled,
          spinner: element.querySelector(".animate-spin") !== null,
          status: status === undefined ? null : status.textContent.trim().toLowerCase(),
        };
      });

    const before = await readRow();
    if (before !== null && before.button === "Stop") {
      // Leave a running tunnel first so this spec observes a start.
      await browser.execute(() => {
        [...document.querySelectorAll(".tunnel-row button")]
          .find((b) => b.textContent.trim() === "Stop")
          ?.click();
      });
      await browser.waitUntil(
        async () => (await readRow())?.button === "Start",
        { timeout: 20000, timeoutMsg: "tunnel never settled to stopped" },
      );
    }

    const clickedAt = Date.now();
    await browser.execute(() => {
      [...document.querySelectorAll(".tunnel-row button")]
        .find((b) => b.textContent.trim() === "Start")
        ?.click();
    });

    // 400ms is comfortably below the 900ms ssh grace window: seeing the
    // label here proves the paint happened *while* the command still runs.
    await browser.waitUntil(
      async () => {
        const now = await readRow();
        return now !== null && now.status === "starting";
      },
      {
        timeout: 400,
        interval: 25,
        timeoutMsg:
          "the optimistic Starting… state never painted within 400ms — a command is blocking the UI thread again",
      },
    );
    const paintedAfterMs = Date.now() - clickedAt;
    const mid = await readRow();
    console.log(
      `[latency] optimistic paint after ${paintedAfterMs}ms: ${JSON.stringify(mid)}`,
    );

    // The mid-flight row must be unmissable: spinner + disabled + label.
    expect(mid?.spinner).toBe(true);
    expect(mid?.disabled).toBe(true);

    // And it must still settle into the truth.
    await browser.waitUntil(
      async () => (await readRow())?.status === "running",
      { timeout: 30000, interval: 250, timeoutMsg: "tunnel never reached running" },
    );
  });
});
