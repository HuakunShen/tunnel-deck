/**
 * Makes the public Uniview JSX augmentation visible to the type checker, so
 * the Solid terminal elements type-check without importing renderer internals.
 */
import type {} from "@uniview/tui-solid/jsx-runtime";
