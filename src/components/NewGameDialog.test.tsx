import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { NewGameDialog } from "./NewGameDialog";

describe("NewGameDialog", () => {
  it("maps AI Starts to a Red human side", async () => {
    const user = userEvent.setup();
    const onStart = vi.fn().mockResolvedValue(undefined);
    render(<NewGameDialog currentDifficulty="medium" canClose onClose={() => undefined} onStart={onStart} />);
    await user.click(screen.getByText("Hard"));
    await user.click(screen.getByText("AI starts"));
    await user.click(screen.getByRole("button", { name: "Begin game" }));
    expect(onStart).toHaveBeenCalledWith("hard", "red");
  });
});

