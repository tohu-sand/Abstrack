// Single keys the window takes from the page and the comment list (never
// from a text field): Delete and Enter for the comment selected on the page
// (the list handles its own), Space for its check mark, Esc to let it go
// (the page first clears selected text), and J and K for the next and the
// previous unchecked comment.

interface Host {
  selectedKey: () => string | null;
  remove: (key: string) => void;
  edit: (key: string) => void;
  toggle: (key: string) => void;
  next: (step: 1 | -1) => void;
  deselect: () => void;
}

export function windowKey(e: KeyboardEvent, host: Host): void {
  if (e.defaultPrevented || e.isComposing || e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
  const active = document.activeElement;
  if (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement || active instanceof HTMLSelectElement) return;
  const onPage = !!active?.classList.contains("paper");
  const inList = !!active?.closest(".list");
  if (!onPage && !inList && active !== document.body) return;
  const key = host.selectedKey();
  const take = (action: () => void) => {
    e.preventDefault();
    action();
  };
  switch (e.key) {
    case "Delete":
      if (onPage && key) take(() => host.remove(key));
      break;
    case "Enter":
      if (onPage && key) take(() => host.edit(key));
      break;
    case " ":
      if (key && !(inList && active instanceof HTMLButtonElement)) take(() => host.toggle(key));
      break;
    case "Escape":
      if (key) take(() => host.deselect());
      break;
    case "j":
    case "J":
      take(() => host.next(1));
      break;
    case "k":
    case "K":
      take(() => host.next(-1));
      break;
  }
}
