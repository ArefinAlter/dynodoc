/** Spreadsheet error values. `value` is the display text, `type` the stable machine name. */
export type ErrorType =
  | 'DIV_BY_ZERO'
  | 'NA'
  | 'NAME'
  | 'NUM'
  | 'REF'
  | 'VALUE'
  | 'NULL'
  | 'CYCLE'
  | 'ERROR'
  | 'SPILL'
  | 'CALC';

const TEXT: Record<ErrorType, string> = {
  DIV_BY_ZERO: '#DIV/0!',
  NA: '#N/A',
  NAME: '#NAME?',
  NUM: '#NUM!',
  REF: '#REF!',
  VALUE: '#VALUE!',
  NULL: '#NULL!',
  CYCLE: '#CYCLE!',
  ERROR: '#ERROR!',
  SPILL: '#SPILL!',
  CALC: '#CALC!'
};

/** Excel's ERROR.TYPE numbering; engine-only errors have none. */
const ERROR_TYPE_NUMBER: Partial<Record<ErrorType, number>> = {
  NULL: 1,
  DIV_BY_ZERO: 2,
  VALUE: 3,
  REF: 4,
  NAME: 5,
  NUM: 6,
  NA: 7,
  SPILL: 9,
  CALC: 14
};

export class FormulaError {
  readonly value: string;
  constructor(
    readonly type: ErrorType,
    readonly message = ''
  ) {
    this.value = TEXT[type];
  }
  toString() {
    return this.value;
  }
  get errorTypeNumber() {
    return ERROR_TYPE_NUMBER[this.type];
  }
}

export const E = {
  DIV0: new FormulaError('DIV_BY_ZERO'),
  NA: new FormulaError('NA'),
  NAME: new FormulaError('NAME'),
  NUM: new FormulaError('NUM'),
  REF: new FormulaError('REF'),
  VALUE: new FormulaError('VALUE'),
  NULL: new FormulaError('NULL'),
  CYCLE: new FormulaError('CYCLE'),
  ERROR: new FormulaError('ERROR'),
  SPILL: new FormulaError('SPILL'),
  CALC: new FormulaError('CALC')
} as const;

const BY_TEXT = new Map<string, FormulaError>(Object.values(E).map((e) => [e.value, e]));

/** Parse an error literal such as `#N/A` (case-insensitive). */
export function errorFromText(text: string): FormulaError | undefined {
  return BY_TEXT.get(text.trim().toUpperCase());
}

export function isError(value: unknown): value is FormulaError {
  return value instanceof FormulaError;
}
