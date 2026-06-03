import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("App", () => {
  it("renders the desktop skills management shell", () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: /Skills Manage/i })).toBeInTheDocument();
    expect(screen.getByRole("searchbox", { name: /Search skills/i })).toBeInTheDocument();
    expect(screen.getAllByText("codex-workflow-guardrails").length).toBeGreaterThan(0);
    expect(screen.getByText("Preview safe mode")).toBeInTheDocument();
  });
});
