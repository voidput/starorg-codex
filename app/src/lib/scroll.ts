/**
 * Scroll <main> (the only scroller; html/body are overflow: clip) so the card with this id is at
 * the top, and focus its title without scrolling anything else. False when the card is missing.
 */
export function jumpToSection(id: string, behavior: ScrollBehavior = 'smooth'): boolean {
  const el = document.getElementById(id);
  const main = document.getElementById('main');
  if (!el || !main) return false;
  const top = main.scrollTop + el.getBoundingClientRect().top - main.getBoundingClientRect().top - 12;
  main.scrollTo({ top, behavior });
  el.querySelector<HTMLElement>('[data-slot=card-title]')?.focus({ preventScroll: true });
  return true;
}
