// `use:autofocus` puts the cursor in a field the moment it appears (a form opened by a click). `use:autofocus={cond}` does it only when `cond` is true.
export function autofocus(el: HTMLElement, on: boolean = true) {
  if (on) el.focus();
}
