import { HyperFormula } from 'hyperformula';

type Input = Record<string, (string | number | boolean)[][]>;
/** Keep the formula graph alive for cell edits. Structural changes use the explicit
 * rebasing path; this cache rebuilds only when sheet names or dimensions change. */
export class WorkbookCalculator {
  private engine?: HyperFormula;
  private previous: Input = {};
  calculate(input: Input) {
    const shape = (data: Input) =>
      Object.entries(data)
        .map(([k, v]) => `${k}:${v.length}:${v[0]?.length || 0}`)
        .join('|');
    if (!this.engine || shape(input) !== shape(this.previous)) {
      this.destroy();
      this.engine = HyperFormula.buildFromSheets(input, { licenseKey: 'gpl-v3' });
    } else {
      const engine = this.engine;
      engine.batch(() => {
        for (const [name, rows] of Object.entries(input)) {
          const sheet = engine.getSheetId(name)!;
          rows.forEach((row, r) =>
            row.forEach((value, c) => {
              if (value !== this.previous[name]?.[r]?.[c])
                engine.setCellContents({ sheet, row: r, col: c }, [[value]]);
            })
          );
        }
      });
    }
    this.previous = input;
    return Object.fromEntries(
      Object.keys(input).map((name) => [
        name,
        this.engine!.getSheetValues(this.engine!.getSheetId(name)!)
      ])
    );
  }
  destroy() {
    this.engine?.destroy();
    this.engine = undefined;
    this.previous = {};
  }
}
