/** Copies a meeting link to the clipboard and says so (or says it could not). */
export async function copyLink(url: string, push: (t: { title: string; tone?: "danger" }) => void) {
  try {
    await navigator.clipboard.writeText(url);
    push({ title: "Meeting link copied" });
  } catch {
    push({ title: "Couldn't copy the link", tone: "danger" });
  }
}
