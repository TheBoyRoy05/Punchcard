// Whole class names, so Tailwind's scanner finds every color.
const PROJECT_COLORS = [
  "pc:bg-project-1",
  "pc:bg-project-2",
  "pc:bg-project-3",
  "pc:bg-project-4",
  "pc:bg-project-5",
  "pc:bg-project-6",
];

export function projectColor(color: number): string {
  return PROJECT_COLORS[color % PROJECT_COLORS.length] ?? "pc:bg-faint";
}
