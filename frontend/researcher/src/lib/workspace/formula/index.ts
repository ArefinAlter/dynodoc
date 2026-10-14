/**
 * Dynodoc spreadsheet formula engine: A1 references, names, structural edits and an
 * Excel-compatible function library, with no third-party runtime dependencies.
 */
export {
  FormulaEngine,
  isValidName,
  parseConstant,
  type CellAddress,
  type CellContent,
  type EngineOptions,
  type NamedExpressionInput
} from './engine';
export { FormulaError, isError, type ErrorType } from './errors';
export { registeredFunctionNames } from './functions';
export { colToName, nameToCol, tryParse } from './parser';
export { formatGeneral } from './numfmt';
export type { Scalar } from './values';
