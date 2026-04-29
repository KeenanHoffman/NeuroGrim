import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
  CardFooter,
} from "./card";

describe("Card", () => {
  it("composes header / title / content / footer", () => {
    render(
      <Card>
        <CardHeader>
          <CardTitle>Title</CardTitle>
          <CardDescription>Desc</CardDescription>
        </CardHeader>
        <CardContent>Body</CardContent>
        <CardFooter>Footer</CardFooter>
      </Card>
    );
    expect(screen.getByText("Title")).toBeInTheDocument();
    expect(screen.getByText("Desc")).toBeInTheDocument();
    expect(screen.getByText("Body")).toBeInTheDocument();
    expect(screen.getByText("Footer")).toBeInTheDocument();
  });

  it("forwards refs", () => {
    let ref: HTMLDivElement | null = null;
    render(
      <Card
        ref={(el) => {
          ref = el;
        }}
        data-testid="card"
      >
        x
      </Card>
    );
    expect(ref).not.toBeNull();
    expect(ref?.tagName).toBe("DIV");
  });

  it("merges custom className", () => {
    render(<Card className="my-custom" data-testid="card">x</Card>);
    expect(screen.getByTestId("card").className).toMatch(/my-custom/);
    // Default classes still present.
    expect(screen.getByTestId("card").className).toMatch(/rounded-lg/);
  });
});
