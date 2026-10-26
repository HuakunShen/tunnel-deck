/**
 * Desktop e2e: the bundled app opens its window and renders the tunnel
 * console. The manager is linked in-process, so these specs need nothing
 * beyond the app itself.
 */
describe("TunnelDeck desktop app", () => {
  it("opens the main window titled TunnelDeck", async () => {
    await browser.waitUntil(
      async () => (await browser.getTitle()).includes("TunnelDeck"),
      { timeout: 30000, timeoutMsg: "main window never titled TunnelDeck" },
    );
    expect(await browser.getTitle()).toContain("TunnelDeck");
  });

  it("renders the console shell: toolbar, sidebar, content", async () => {
    const toolbar = await browser.$(".tunnel-toolbar");
    await toolbar.waitForExist({ timeout: 30000 });
    expect(await toolbar.isDisplayed()).toBe(true);

    const sidebar = await browser.$(".tunnel-sidebar");
    expect(await sidebar.isDisplayed()).toBe(true);

    const content = await browser.$(".tunnel-content");
    expect(await content.isDisplayed()).toBe(true);
  });

  it("shows the New tunnel action", async () => {
    const action = await browser.$("button=New tunnel");
    await action.waitForExist({ timeout: 30000 });
    expect(await action.isDisplayed()).toBe(true);
  });

  it("publishes the native material and makes it adjustable", async () => {
    // The host only publishes a backdrop after the material really installed,
    // and only a little after launch: publishing earlier would deadlock the
    // webview that is still starting.
    // This service resolves `execute` to the bare value, not a { value } envelope.
    await browser.waitUntil(
      async () =>
        (await browser.execute(() => document.documentElement.dataset.backdrop ?? "")) !== "",
      { timeout: 30000, timeoutMsg: "no backdrop was ever published" },
    );
    const backdrop = await browser.execute(() => document.documentElement.dataset.backdrop);
    // A browser has nothing to be translucent over, so this must not silently
    // degrade to "none" in the packaged app.
    expect(backdrop).toMatch(/^(glass|vibrancy)$/);

    // One knob drives every surface, so moving it must move them together.
    const read = async () =>
      await browser.execute(() =>
        getComputedStyle(document.documentElement).getPropertyValue("--material-opacity").trim(),
      );
    const before = await read();
    expect(before).not.toBe("");
    await browser.execute(() => document.documentElement.style.setProperty("--material-opacity", "0.5"));
    expect(await read()).toBe("0.5");
  });

  it("captures the console window for the gallery", async () => {
    // Best-effort wait for populated cards (daemon data may lag first
    // paint), then screenshot regardless — the image itself is the evidence
    // of what the window shows.
    await browser
      .waitUntil(
        async () => (await browser.$$(".tunnel-row")).length > 0,
        { timeout: 30000, timeoutMsg: "no tunnel rows rendered" },
      )
      .catch(() => undefined);
    // WDIO screenshots the WebView directly (not the system compositor),
    // so other windows on screen cannot cover it. Path is relative to the
    // package dir (apps/desktop); land in the repo-level gallery.
    await browser.saveScreenshot("../../screenshots/desktop-app.png");
  });
});
