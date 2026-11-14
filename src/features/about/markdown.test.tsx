import { render, screen } from "@testing-library/react";
import { Markdown } from "./markdown";

describe("Markdown", () => {
  it("renders headings, paragraphs, lists and emphasis", () => {
    render(<Markdown source={"# Title\n\n## Part\n\nSome **bold** and *soft* text\ncontinued.\n\n- one\n- two **b**\n"} />);
    expect(screen.queryByText("Title")).toBeNull(); // the page supplies its own title
    expect(screen.getByRole("heading", { name: "Part" })).toBeInTheDocument();
    expect(screen.getByText("bold")).toBeInTheDocument();
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    expect(screen.getByText(/Some/).textContent).toContain("text continued.");
  });

  it("never turns text into HTML", () => {
    const { container } = render(<Markdown source={'<script>alert(1)</script> <img src=x onerror=alert(1)> [link](javascript:alert(1))'} />);
    expect(container.querySelector("script")).toBeNull();
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("a")).toBeNull();
    expect(container.textContent).toContain("<script>alert(1)</script>");
  });
});
