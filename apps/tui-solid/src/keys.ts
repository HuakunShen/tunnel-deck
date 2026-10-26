/**
 * Terminal input → model actions. Named keys (arrows, Enter, Escape, Ctrl-C)
 * arrive as `key` events and plain letters as `text` events, so both are
 * normalised to one token before dispatch. Returns whether the event was used.
 */
import type { TuiInputEvent } from "@uniview/tui-core";
import type { TunnelModel } from "./model.js";

export interface Controls {
  readonly model: TunnelModel;
  readonly quit: () => void;
}

function token(event: TuiInputEvent): string {
  if (event.type === "text") return event.text;
  if (event.type === "key") return event.ctrl && event.key === "c" ? "ctrl-c" : event.key;
  return "";
}

export function handleKey(controls: Controls, event: TuiInputEvent): boolean {
  const { model, quit } = controls;
  const pressed = token(event);
  if (pressed === "") return false;
  if (pressed === "ctrl-c") {
    quit();
    return true;
  }

  if (model.screen() === "log") {
    if (pressed === "Escape" || pressed === "q") model.closeLog();
    return true;
  }

  // Deleting needs the same key twice in a row; anything else disarms it.
  if (pressed !== "d") model.cancelDelete();

  switch (pressed) {
    case "q":
      quit();
      return true;
    case "ArrowUp":
    case "k":
      model.move(-1);
      return true;
    case "ArrowDown":
    case "j":
      model.move(1);
      return true;
    case "Enter":
      void model.toggle();
      return true;
    case "p":
      void model.probe();
      return true;
    case "s":
      void model.stopAll();
      return true;
    case "d":
      void model.requestDelete();
      return true;
    case "l":
      void model.openLog();
      return true;
    case "r":
      void model.refresh();
      return true;
    default:
      return false;
  }
}
