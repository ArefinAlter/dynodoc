// Importing the modules registers their functions in the shared table.
import './fn-math';
import './fn-agg';
import './fn-text';
import './fn-logic';
import './fn-lookup';
import './fn-date';
import './fn-finance';
import './fn-dist';
import './fn-complex';
import './fn-bessel';
import './fn-extra';
import './fn-convert';
import { FUNCTIONS } from './fn-util';

/**
 * Names that HyperFormula registered next to the standard Excel ones. They map to the same
 * implementations so workbooks written against the previous engine keep calculating.
 */
const ALIASES: Record<string, string> = {
  COVARIANCEP: 'COVARIANCE.P',
  COVARIANCES: 'COVARIANCE.S',
  SKEWP: 'SKEW.P',
  WEIBULLDIST: 'WEIBULL.DIST',
  TINV2T: 'T.INV.2T',
  TDISTRT: 'T.DIST.RT',
  TDIST2T: 'T.DIST.2T',
  FINVRT: 'F.INV.RT',
  FDISTRT: 'F.DIST.RT',
  CHIDISTRT: 'CHISQ.DIST.RT',
  CHIINVRT: 'CHISQ.INV.RT',
  LOGNORMINV: 'LOGNORM.INV',
  POISSONDIST: 'POISSON.DIST'
};
for (const [alias, target] of Object.entries(ALIASES)) {
  const def = FUNCTIONS.get(target);
  if (def && !FUNCTIONS.has(alias)) FUNCTIONS.set(alias, def);
}

export { FUNCTIONS };

/** Public function names for pickers; internal aliases and prefixed forms are excluded. */
export function registeredFunctionNames(): string[] {
  return [...FUNCTIONS.keys()].sort();
}
