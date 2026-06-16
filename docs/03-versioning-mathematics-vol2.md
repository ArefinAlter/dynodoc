# The Mathematics, Science & Engineering of the Research IDE Versioning Protocol

### A rigorous companion — formal foundations, equations, algorithms, complexity, and the patterns we reject and why

**Status:** Deep theoretical & engineering treatment
**Companion to:** `research_ide_versioning_protocol.md` (the design narrative). That document argued *what* to build and *why*; this one proves the claims, states the mathematics formally, gives the algorithms in full, analyzes their costs, and dissects the expensive patterns we deliberately avoid.

**Reading contract.** Every non-trivial equation is followed by a plain-language gloss of *what each symbol means and why the equation says something we care about.* Diagrams are given as labeled ASCII (portable inside the document) and supplemented by rendered figures in the chat. Pseudocode is given for every algorithm we would actually implement. Complexity is stated in $O(\cdot)$ with the reasoning, not asserted.

---

## Notation used throughout

| Symbol | Meaning |
|---|---|
| $a \to b$ | event $a$ *happened-before* event $b$ (causal precedence) |
| $a \parallel b$ | $a$ and $b$ are *concurrent* (no causal order either way) |
| $\sqcup$ | *join* — least upper bound in a lattice; also the CRDT merge |
| $\sqsubseteq$ | the partial order of a lattice ("below or equal in information") |
| $\circ$ | sequential composition of operations: $a \circ b$ = "do $a$, then $b$" |
| $T(o_x, o_y)$ | operational transform of $o_x$ against $o_y$ |
| $\mathbb{N}^n$ | the set of length-$n$ vectors of naturals (vector clocks) |
| $|T|$ | number of nodes in tree $T$ |
| $\bot, \top$ | bottom (least) and top (greatest) lattice elements |

---

# Part A — Foundations: order, time, and the limits of the possible

Everything downstream — CRDTs, the merge engine, the audit log — rests on three mathematical pillars: **order theory** (which gives us a precise meaning of "merge"), **the theory of logical time** (which gives us a precise meaning of "concurrent"), and **the impossibility results** (which tell us what we are *forbidden* from having and therefore must design around). We build these first because skipping them is exactly what made the previous document hand-wavy.

## A.1 Order theory: what "merge" actually means

### A.1.1 Partial orders

A **partial order** (poset) is a set $S$ with a relation $\sqsubseteq$ satisfying, for all $a,b,c \in S$:

$$
a \sqsubseteq a \quad\text{(reflexive)}
$$
$$
a \sqsubseteq b \;\wedge\; b \sqsubseteq a \;\Rightarrow\; a = b \quad\text{(antisymmetric)}
$$
$$
a \sqsubseteq b \;\wedge\; b \sqsubseteq c \;\Rightarrow\; a \sqsubseteq c \quad\text{(transitive)}
$$

*Explanation.* Read $a \sqsubseteq b$ as "$a$ is below-or-equal-to $b$," or for our purposes "$b$ contains at least as much information as $a$." Reflexivity says every state contains its own information. Antisymmetry says two states that each contain the other's information are the same state. Transitivity says information accumulation chains. The word *partial* is the crucial one: unlike numbers, **not every pair need be comparable.** Two replicas that have each seen edits the other hasn't are *incomparable* — neither $\sqsubseteq$ the other. That incomparability is precisely the formal shape of "they have diverged," and it is why a total order (like a wall clock) is the wrong tool.

### A.1.2 Join-semilattices — the algebra of convergence

A **join-semilattice** is a poset in which *every* pair $a,b$ has a **least upper bound** (the **join**, written $a \sqcup b$): an element that is above both and below every other element that is above both. Formally $a \sqcup b$ is defined by:

$$
a \sqsubseteq a \sqcup b \;\wedge\; b \sqsubseteq a \sqcup b \quad\text{(upper bound)}
$$
$$
\forall u.\;\big(a \sqsubseteq u \wedge b \sqsubseteq u\big) \Rightarrow a \sqcup b \sqsubseteq u \quad\text{(least such)}
$$

*Explanation.* The first line: the join sits above both inputs — it contains all the information of $a$ and all of $b$. The second line: it contains *nothing extra* — it is the smallest state that does so. This is exactly what we want a merge to be: combine two divergent states into the unique smallest state that includes both, inventing no information that wasn't in one of them.

The join operation, derived from the order, provably satisfies three algebraic laws — and these three laws are the entire reason CRDTs work:

$$
a \sqcup a = a \qquad\text{(idempotent)}
$$
$$
a \sqcup b = b \sqcup a \qquad\text{(commutative)}
$$
$$
(a \sqcup b) \sqcup c = a \sqcup (b \sqcup c) \qquad\text{(associative)}
$$

*Explanation, and why each law buys us a specific real-world guarantee — this mapping is the heart of the whole subject:*

- **Idempotent** ($a \sqcup a = a$): merging a state with itself changes nothing. *Consequence:* if the network delivers the same update **twice** (which unreliable networks do constantly), re-merging is harmless. We need no deduplication.
- **Commutative** ($a \sqcup b = b \sqcup a$): merge order doesn't matter. *Consequence:* if replica X's update reaches Z before Y's, while at W they arrive in the opposite **order**, both still converge. We need no ordering guarantee from the network.
- **Associative** ($(a\sqcup b)\sqcup c = a\sqcup(b\sqcup c)$): grouping doesn't matter. *Consequence:* updates can be **batched** arbitrarily — sync three at once or one at a time — with identical results.

Together: a merge built from a semilattice join is immune to the three things a real network does to messages — **duplication, reordering, and batching.** A data type whose states form a semilattice and whose merge is the join cannot diverge regardless of network misbehavior. That single sentence is the foundation of conflict-free replication, and it is why Part B's CRDTs are *defined* as semilattices rather than merely happening to be implemented as them.

We can also recover the order from the join: $a \sqsubseteq b \iff a \sqcup b = b$. ("$a$ is below $b$ exactly when merging $a$ into $b$ adds nothing.")

### A.1.3 A picture: the Hasse diagram of converging replicas

```
                    {e1,e2,e3}          ← the join (lub): all edits, the converged state
                   /     |     \
                  /      |      \
          {e1,e2}     {e1,e3}    {e2,e3}   ← partially-synced replicas (incomparable to each other)
              \    \   /    \   /    /
               \    \ /      \ /    /
               {e1}  {e2}    {e3}          ← replicas after one local edit each
                  \    |     /
                   \   |    /
                    \  |   /
                      { }  ⊥               ← the common ancestor (bottom)
```

*Explanation.* Each node is a possible replica state = the *set of edits it has seen*. Lines go upward to states with strictly more information. Three replicas each make one edit ($\{e_1\}, \{e_2\}, \{e_3\}$) — mutually **incomparable** (diverged). As they sync pairwise they climb. The unique top $\{e_1,e_2,e_3\}$ is the join — the state every replica reaches once all edits propagate, *no matter the order or grouping of syncs.* Convergence is literally "everyone climbs to the same lub." This diagram is the abstract shape of every figure in this document; keep it in mind.

## A.2 Logical time: a precise definition of "concurrent"

A wall clock cannot tell us whether two edits conflict, because two devices' clocks are not comparable to the precision we need (NTP synchronization error is typically tens of milliseconds, and clocks can run *backwards* on adjustment; two field edits seconds apart are unorderable by wall time, and an edit at 14:00:00.05 on a fast clock may have *really* happened after one stamped 14:00:00.10 on a slow one). We need *logical* time. This is Lamport's contribution (1978), and it is foundational.

### A.2.1 The happened-before relation

Define $\to$ (happened-before) as the smallest relation satisfying:

$$
\textbf{(L1)}\quad a,b \text{ in same replica}, a \text{ before } b \;\Rightarrow\; a \to b
$$
$$
\textbf{(L2)}\quad a = \text{send}(m),\; b = \text{receive}(m) \;\Rightarrow\; a \to b
$$
$$
\textbf{(L3)}\quad a \to b \;\wedge\; b \to c \;\Rightarrow\; a \to c \quad(\text{transitive})
$$

Two events are **concurrent**, written $a \parallel b$, iff neither precedes the other:

$$
a \parallel b \;\;\stackrel{\text{def}}{\iff}\;\; \neg(a \to b) \;\wedge\; \neg(b \to a)
$$

*Explanation.* (L1) local program order; (L2) a message must be sent before received (the only cross-replica causal link); (L3) causality chains. The definition of $\parallel$ is the one that matters for us: **concurrent edits are exactly the ones with no causal path connecting them** — Aluk's edit in Chittagong and a co-PI's edit in Toronto, made while both offline, are concurrent because no message linked them. These — and *only* these — are the edits a merge or review policy must adjudicate. Everything else has a definite causal order the system can simply respect. $\to$ is a **strict partial order** (irreflexive, transitive); concurrency is its incomparability relation, tying directly back to A.1.1.

### A.2.2 Lamport clocks and their fundamental limitation

Assign each event an integer timestamp $C(a)$ by the rule: each replica $P_i$ keeps a counter $C_i$, and

$$
\text{before each local event:}\quad C_i \leftarrow C_i + 1
$$
$$
\text{on send, attach } t = C_i; \quad\text{on receive of } t:\quad C_i \leftarrow \max(C_i, t) + 1
$$

This guarantees the **clock condition**:

$$
a \to b \;\Rightarrow\; C(a) < C(b)
$$

*Explanation.* Increment before events so each gets a fresh number; on receiving a message, jump ahead of the sender's clock so the receive is numbered after the send. The guarantee: causally-ordered events get increasing numbers. **But the converse is false:**

$$
C(a) < C(b) \;\;\not\Rightarrow\;\; a \to b
$$

Two concurrent events can get any numbers at all, including $C(a) < C(b)$ while $a \parallel b$. *Consequence:* a Lamport timestamp gives a usable **total order** (break ties by replica id) — which we exploit in the move algorithm (Part C) — but it **destroys the information of which events were truly concurrent.** For *detecting* conflicts we need more, which is the next structure.

### A.2.3 Vector clocks: detecting concurrency exactly

Give each of $n$ replicas a vector $V \in \mathbb{N}^n$. Replica $P_i$ updates by:

$$
\text{before local event:}\quad V_i[i] \leftarrow V_i[i] + 1
$$
$$
\text{on receive of vector } V_m:\quad V_i[k] \leftarrow \max\!\big(V_i[k],\, V_m[k]\big)\;\;\forall k,\;\; \text{then } V_i[i] \leftarrow V_i[i]+1
$$

Compare vectors componentwise:

$$
V(a) \le V(b) \;\;\stackrel{\text{def}}{\iff}\;\; \forall k.\; V(a)[k] \le V(b)[k]
$$
$$
V(a) < V(b) \;\;\stackrel{\text{def}}{\iff}\;\; V(a) \le V(b) \;\wedge\; V(a) \ne V(b)
$$

Then — and this is the payoff — causality is captured **exactly** (the converse now holds, unlike Lamport):

$$
a \to b \;\;\iff\;\; V(a) < V(b)
$$
$$
a \parallel b \;\;\iff\;\; \neg\big(V(a) \le V(b)\big) \;\wedge\; \neg\big(V(b) \le V(a)\big)
$$

*Explanation.* Component $V_i[k]$ counts "how many events from replica $k$ has $P_i$ seen." Position $i$ counts your own events; on a message you take the componentwise $\max$ (absorbing everything the sender had seen) then tick your own. Now $V(a) < V(b)$ means $b$'s replica had seen *strictly more* history, including $a$ — genuine causal precedence. $a \parallel b$ means **each vector has a coordinate where it leads** — each saw something the other hadn't — the formal signature of divergence. The set of vector clocks under $\le$ forms exactly the kind of partial order (indeed a lattice) of A.1, with $\max$ as the join. **This is the structure that lets the system know it must ask a human, versus silently order two edits.** It is also why Part B's registers carry version vectors: to distinguish "this overwrites that" (comparable) from "these clash" (concurrent).

**Worked numeric example.** Three replicas; "·" = not yet ticked.

```
P1: e1[1,0,0] ──send──────────────► P2 receives → e3 [1,1,0]
P2:               e2[0,1,0]                          (P2 had its own e2 then learned e1)
P3:                                   e4[0,0,1]

Compare:  V(e1)=[1,0,0]  V(e2)=[0,1,0]     neither ≤ other  ⇒  e1 ∥ e2  (concurrent)
          V(e1)=[1,0,0]  V(e3)=[1,1,0]     [1,0,0] < [1,1,0] ⇒  e1 → e3  (causal)
          V(e2)=[0,1,0]  V(e4)=[0,0,1]     neither ≤ other  ⇒  e2 ∥ e4  (concurrent)
```

*Reading it:* $e_1$ and $e_2$ are concurrent (made independently before any sync) — a merge must consider both. $e_1 \to e_3$ because $e_3$ is the receive event that learned $e_1$ — the system simply keeps $e_3$ after $e_1$, no adjudication. The vectors *compute* this with two integer comparisons.

**Cost note.** A naive vector clock is $O(n)$ space per stamped event for $n$ replicas, which is the well-known scaling weakness. For our setting $n$ is tiny (a research team of single digits, not thousands), so the cost is negligible; at larger $n$ one uses compression (interval/dotted version vectors) — flagged here for completeness, not because it bites us.

## A.3 Consistency models and the impossibility results

We now have order ($\sqcup$) and time ($\to, \parallel$). The last foundation is *what consistency is even achievable*, because this is dictated by impossibility theorems, not by cleverness.

### A.3.1 The CAP theorem, stated and sketched

**Theorem (Brewer's conjecture; Gilbert & Lynch 2002, informal statement).** A distributed data store cannot simultaneously provide all three of:

- **(C) Consistency** (linearizability — every read sees the latest write),
- **(A) Availability** (every request to a non-failed node gets a response),
- **(P) Partition tolerance** (the system keeps working when the network drops messages between nodes).

When a partition occurs (P), one must choose C or A.

*Proof sketch (the two-sided partition argument).* Suppose nodes $G_1, G_2$ are partitioned (no messages cross). A client writes $v$ to $G_1$. A client then reads from $G_2$. If the system is **available**, $G_2$ must answer without hearing from $G_1$ — so it returns a stale value, **violating consistency**. If instead the system insists on **consistency**, $G_2$ must refuse to answer until the partition heals, **violating availability**. No third option exists during the partition. $\blacksquare$

*Explanation and why it dictates our architecture.* Our users go offline *by design* — a field enumerator in a village with no signal *is* a network partition, deliberately and routinely. P is therefore not optional. CAP then forces a binary choice, and for an authoring tool the choice is forced again: a researcher who cannot edit because a server is unreachable is a broken product, so we **must** choose **A** over **C**. Choosing A means abandoning the fantasy of a single always-correct authoritative copy and embracing *optimistic replication*: let replicas diverge, then converge. The strongest convergence guarantee compatible with this choice is the subject of A.3.2. **CAP is not a constraint we work within reluctantly; it is the theorem that selects the entire CRDT/local-first family for us.**

### A.3.2 Strong Eventual Consistency (SEC), defined

Since we cannot have linearizability under partition, we target the strongest thing we can. Following Shapiro et al. (2011), a replicated object is **Strongly Eventually Consistent** iff:

$$
\textbf{(Eventual delivery)}\quad \text{an update delivered at some correct replica is eventually delivered at all correct replicas}
$$
$$
\textbf{(Strong convergence)}\quad \text{any two replicas that have delivered the same \emph{set} of updates have equal state}
$$
$$
\textbf{(Termination)}\quad \text{all method executions terminate}
$$

*Explanation.* Contrast with plain *eventual* consistency (the weaker, older notion), which promises replicas converge "eventually" but may require **conflict resolution / rollback** to do so — exactly the manual merge pain we are trying to abolish. **Strong** convergence is sharper: *same set of updates ⇒ identical state, full stop, with no resolution step.* The word "set" is doing heavy lifting — it means **order and multiplicity of delivery are irrelevant**, which is precisely what the semilattice laws of A.1.2 (commutative, idempotent, associative) guarantee. SEC is therefore not an aspiration; it is a *theorem* about any data type whose states form a semilattice merged by join. That is the bridge from Part A to Part B: **SEC ⟸ semilattice structure**, and CRDTs are the data types engineered to have that structure.

### A.3.3 The two roads to SEC

There are exactly two disciplined ways to obtain strong convergence, and they correspond to the two CRDT families formalized in Part B:

$$
\textbf{State-based (CvRDT):}\quad \text{states form a join-semilattice; } \mathrm{merge} = \sqcup. \text{ Convergence} \Leftarrow \text{A.1.2 laws.}
$$
$$
\textbf{Operation-based (CmRDT):}\quad \text{concurrent operations commute, delivered in causal order. Convergence} \Leftarrow \text{commutativity.}
$$

*Explanation.* The state-based road gets convergence from the *merge function's* algebra (the join's three laws). The op-based road gets it from the *operations'* algebra: if whenever $a \parallel b$ we have $a \circ b = b \circ a$ (they commute), then all replicas applying the same concurrent ops in any locally-chosen order reach the same state. Both roads reach SEC; they differ in what they put on the wire (whole states vs. operations) and what they demand of the network (nothing vs. causal delivery). Part B develops both and explains which we use where — and Part D's central simplification question is whether our *semantic event log* can be made an op-based CRDT by proving its operations commute on disjoint nodes.

---

*Part A has established: merge = semilattice join (A.1); "concurrent" = vector-clock incomparability (A.2); we are forced by CAP to choose availability and therefore optimistic replication (A.3.1); the best we can then get is SEC, which follows for free from semilattice structure (A.3.2–3). Part B now builds the actual data types on these foundations, with their convergence proofs, their costs, and the OT and patch-theory alternatives we reject — with the mathematics of why.*
---

# Part B — The data types: CRDTs, and the alternatives we reject

Part A proved that semilattice structure yields SEC. Part B builds the concrete data types that have that structure, proves they converge, analyzes their costs, and then examines the two rival lineages — operational transformation and patch algebra — with the mathematics of *why* each is more expensive or more fragile than the CRDT road for our problem.

## B.1 State-based CRDTs (CvRDTs) and the convergence theorem

### B.1.1 Definition

A **CvRDT** is a triple $(S, \sqsubseteq, \sqcup)$ where $(S,\sqsubseteq)$ is a join-semilattice, $\sqcup$ is the join, and every local **update** $u: S \to S$ is **inflationary**:

$$
\forall s \in S.\quad s \sqsubseteq u(s)
$$

*Explanation.* "Inflationary" means an update never moves the state *down* the lattice — it only ever adds information (climbs, or stays put). A counter only increases; a set only grows; a register's version only advances. This is the discipline that makes the climbing picture of A.1.3 valid: every local edit goes up, and merges go up to joins, so the global motion is monotone toward the lub.

### B.1.2 The convergence theorem (with proof)

**Theorem.** Let replicas each start at $\bot$, apply local inflationary updates, and periodically merge by $\sqcup$. If every update eventually reaches every replica (directly or transitively through merges), all replicas converge to

$$
\bigsqcup_{i} s_i
$$

the join of all states ever produced — and this holds regardless of the order, grouping, or repetition of merges.

*Proof.* Let replica $R$ have merged a multiset $M$ of states. By **commutativity** and **associativity**, $\bigsqcup M$ is independent of the order and grouping in which $R$ performed the merges. By **idempotence**, $\bigsqcup M$ is independent of how many times each state appeared in $M$ — so $\bigsqcup M$ depends only on the *set* $\mathrm{set}(M)$ of distinct states. Once every state has propagated everywhere, every replica's set is the same total set $\{s_1, s_2, \dots\}$, hence every replica computes the same join $\bigsqcup_i s_i$. $\blacksquare$

*Explanation.* This is just A.1.2's three laws cashed out: commutativity kills order-dependence, associativity kills grouping-dependence, idempotence kills duplication-dependence. The proof is three sentences because the algebra did the work. **This is the entire reason we can let field devices sync in any order, over flaky links, with retransmissions, and still guarantee identical questionnaires.** It is also why we reach for CRDTs rather than inventing merge logic: the guarantee is structural, not heuristic.

### B.1.3 The cost of the pure state-based road, and the delta fix

The naive CvRDT ships the **whole state** $s$ on every sync. If the questionnaire CRDT state is $|s|$ bytes, each sync costs $O(|s|)$ bandwidth *even for a one-character edit*. For a growing instrument this is wasteful:

$$
\text{naive sync cost over } k \text{ edits} \;=\; O(k \cdot |s|) \quad\text{(ships full state each time)}
$$

**Delta-state CRDTs** (Almeida, Shoker, Baquero 2018) fix this by shipping only a **delta** $\delta$ — a small join-semilattice element representing just the change — such that $\mathrm{merge}(s, \delta)$ has the same effect as the full update, while preserving the three laws. Cost drops to:

$$
\text{delta sync cost over } k \text{ edits} \;=\; O\!\Big(\textstyle\sum_i |\delta_i|\Big) \;\approx\; O(\text{total change}), \quad |\delta_i| \ll |s|
$$

*Explanation and design consequence.* Pure state-based shipping is an **expensive pattern** we avoid: it makes bandwidth scale with document size rather than edit size, which on a 2G field connection is the difference between usable and not. We either use delta-state CRDTs or (more likely) an **operation-based** formulation (B.2), which is naturally delta-sized. The lesson recurs throughout Part C's anti-patterns: *the cost that matters is per-edit, and any design whose per-edit cost scales with total document size is suspect.*

## B.2 Operation-based CRDTs (CmRDTs)

### B.2.1 Definition and convergence condition

A **CmRDT** broadcasts **operations** rather than states. Operations are delivered in **causal order** (if $\mathrm{op}_a \to \mathrm{op}_b$ then every replica applies $a$ before $b$ — enforceable with the vector clocks of A.2.3). The data type converges iff **concurrent operations commute**:

$$
\forall\, \mathrm{op}_a \parallel \mathrm{op}_b:\quad \mathrm{op}_a \circ \mathrm{op}_b \;=\; \mathrm{op}_b \circ \mathrm{op}_a
$$

*Explanation.* Causal delivery handles all *ordered* pairs (they're applied in their causal order everywhere). The only freedom left is *concurrent* pairs, and if those commute, then it does not matter which order each replica happens to apply them — the result is identical. Convergence again reduces to an algebraic law, this time on operations rather than on a merge function. Causal delivery is weaker than the total order OT needs (B.3) and is exactly deliverable by attaching vector clocks.

### B.2.2 Why this is the family we build on

CmRDTs are delta-sized by nature (an operation is small), need only causal delivery (cheap, no central referee — contrast OT's total-order server), and map directly onto our **semantic event log**: an "operation" *is* a semantic event (`PromptEdited`, `ChoiceAdded`, …). The pivotal design question of Part D is then sharp and answerable: *do our semantic events, restricted to operations on disjoint nodes, commute?* If yes, the event log is itself a CmRDT and the separate convergence library collapses into it. We will show (Part D) that disjoint-node operations commute trivially, and that the only non-commuting pairs are same-node concurrent writes — which we deliberately route to human adjudication anyway (the register-conflict decision of the design doc). This is the deep justification for that design choice: *we surface exactly the operation pairs that violate commutativity, and no others.*

## B.3 Sequence CRDTs: ordering under concurrency

Ordered lists — sections within a form, items within a section, choices within an item, characters within a prompt — are the one genuinely subtle CRDT, because "insert at position 3" is not stable under concurrency.

### B.3.1 The problem, formally

Let a list have elements at integer indices. Replica $X$ does $\mathrm{insert}(\text{`a'}, 3)$; concurrently replica $Y$ does $\mathrm{insert}(\text{`b'}, 3)$. Applying $X$ then $Y$ versus $Y$ then $X$ gives different orders, and worse, $Y$'s "index 3" no longer denotes the slot $Y$ intended after $X$'s insert shifts everything. Integer indices are **not** a commutative basis. We need positions that are *stable identifiers*, not shifting offsets.

### B.3.2 Dense identifiers and the "between" operation

The family of solutions (WOOT, Logoot, LSEQ, Treedoc, RGA, YATA) all assign each element a globally unique identifier drawn from a **densely totally ordered** set $(\mathcal{P}, <)$ with the property:

$$
\forall\, p, q \in \mathcal{P},\; p < q \;\;\Rightarrow\;\; \exists\, r \in \mathcal{P}.\; p < r < q
$$

*Explanation.* "Dense" means between any two positions a fresh one always exists — so an insert names its target as "$\mathrm{between}(p, q)$" (a stable claim that survives concurrent inserts) rather than "index 3" (a fragile offset). Concurrent inserts get distinct identifiers and a deterministic order; nothing shifts; commutativity (B.2.1) is restored. The identifier set is typically rationals, or variable-length strings over an alphabet ordered lexicographically.

### B.3.3 RGA (Replicated Growable Array), the workhorse

RGA (Roh et al. 2011; used by Automerge) represents the list as a linked structure where each element carries a unique timestamp $t$ (a Lamport timestamp, A.2.2), and an insert specifies the id of the element it follows. Concurrent inserts after the **same** predecessor are ordered by **descending timestamp**:

$$
\text{among elements sharing a predecessor, order by } t \text{ (newest first), ties by replica id}
$$

*Explanation.* Using the predecessor's *identity* (not its index) makes the insert position stable; using timestamp order for same-predecessor concurrent inserts makes the resolution deterministic and identical on all replicas (hence convergent), and the Lamport total order guarantees a definite winner. RGA additionally has a *proven no-interleaving* property (see B.3.4). YATA (Yjs) uses a closely related scheme tuned for the common case of sequential typing.

### B.3.4 The interleaving anomaly (a concrete failure of naive schemes)

Some sequence-CRDT schemes can **interleave** concurrent runs. Suppose the list is empty and:

$$
X \text{ inserts the run } [\,H,\,e,\,l,\,l,\,o\,]; \qquad Y \text{ concurrently inserts } [\,W,\,o,\,r,\,l,\,d\,]
$$

A correct scheme converges to one run entirely before the other (`HelloWorld` or `WorldHello`). A naively-designed scheme can converge to a **character-level interleaving** like:

$$
[\,H,\,W,\,e,\,o,\,l,\,r,\,l,\,l,\,o,\,d\,] = \texttt{"HWeolrllod"}
$$

*Explanation.* Interleaving is convergent (all replicas agree on the garbage) but violates **intention** — neither author meant that. For us this would corrupt a prompt or a choice label under concurrent editing. RGA and the OpSets-specified types come with proofs that this cannot happen; this is a correctness property we *require* of whatever sequence CRDT we adopt, and a reason not to hand-roll one casually. It is listed as a worked anomaly because "we use a sequence CRDT" is not enough — *which* one, and *with the no-interleaving guarantee*, matters.

### B.3.5 Fractional indexing: the cheap approximation, and its precise limits

For short lists edited infrequently (our items/choices), a full sequence CRDT is heavier than needed. **Fractional indexing** assigns each element an order key $k \in (\,0,1\,)$ (or a string), inserting between neighbors $a<b$ at:

$$
k_{\text{new}} = \frac{a+b}{2} \quad\text{(rational form)}
$$

*Explanation and the catch.* One reorder rewrites *one* key — cheap and legible. But the rational midpoint's **denominator doubles** each time you insert at the same spot: after $m$ adversarial same-slot inserts the key needs $\Theta(m)$ bits:

$$
\text{key precision after } m \text{ same-position inserts} = \Theta(m) \text{ bits}
$$

String-based keys (base-$N$) behave similarly: average key length grows $\sim 1$ symbol per adversarial insert, requiring periodic **rebalancing** ($O(L)$ to renumber a list of length $L$). And — the real limitation — **two concurrent inserts at the identical position must break ties by replica id, which can interleave** (B.3.4). So fractional indexing is *not* a full sequence CRDT; it is a sound choice precisely when same-slot concurrent insertion is rare, which holds for a research team editing tens of items. We use it for item/choice ordering, keep a sequence CRDT in reserve per-list, and use a *proper* text CRDT for the genuinely co-typed prompt fields. This calibration — cheap structure where contention is rare, heavy structure only where it isn't — is a recurring engineering theme.

## B.4 Tombstones and the space cost of deletion

### B.4.1 Why deletes cannot truly delete

In a sequence CRDT, an element's unique id may be referenced by a concurrent operation still in flight (e.g. an insert "after" the element being deleted). Physically removing it would make that concurrent op undefined. So deletes are **tombstones**: the element is marked dead but retained. Space grows with *total deletions ever*, not live size:

$$
\text{state size} = \Theta(\text{live elements} + \text{tombstones}) = \Theta(\text{total elements ever created})
$$

### B.4.2 When a tombstone can be collected: causal stability

A tombstone is safe to physically remove once **no future operation can reference it** — i.e. once every replica has seen the delete, so no concurrent insert-after can still arrive. This is **causal stability**: an operation $o$ is causally stable when every replica's vector clock dominates $o$'s:

$$
o \text{ is causally stable} \iff \forall\, \text{replicas } R.\; V_R \ge V(o)
$$

*Explanation.* Once *everyone's* clock is at or beyond $o$, nothing concurrent with $o$ can still be in flight, so $o$'s tombstones are inert and collectable. Detecting this needs the vector-clock bookkeeping of A.2.3. *Design consequence:* tombstone growth is the classic CRDT space leak and a real **cost of long-lived documents**. For our small, finite-lifetime instruments the absolute growth is trivial, but we still implement causal-stability GC rather than ignore it — because "small" is an assumption that a decade-long living instrument would violate, and the cost of the safety valve is low.

## B.5 The rejected lineage I: Operational Transformation — and the mathematics of its cost

OT is the other way to get convergent real-time editing. We reject it as a foundation; here is the formal reason, not just the assertion.

### B.5.1 The transformation function and TP1

OT represents edits as operations against positions and reconciles concurrency with a transformation function $T$. For two concurrent ops $o_1 \parallel o_2$ produced against the *same* state, convergence requires **Transformation Property 1**:

$$
\textbf{(TP1)}\qquad o_1 \circ T(o_2, o_1) \;=\; o_2 \circ T(o_1, o_2)
$$

*Explanation.* Read $T(o_2, o_1)$ as "$o_2$ rewritten so it still means what its author intended, *after* $o_1$ has been applied." TP1 says: the state reached by *"do $o_1$, then the adjusted $o_2$"* equals the state reached by *"do $o_2$, then the adjusted $o_1$."* This is the diamond that forces two replicas — one of which saw $o_1$ first, the other $o_2$ first — to converge. If $o_1$ inserts at 5 and $o_2$ inserts at 3, then $T(o_1,o_2)$ bumps $o_1$ to position 6 (because $o_2$ shifted everything after position 3), and the two application orders land in the same place. So far, manageable.

### B.5.2 TP2, and why it is the expensive, fragile part

With **three or more** concurrent operations, transformations are composed along different paths, and consistency requires **Transformation Property 2**:

$$
\textbf{(TP2)}\qquad T\big(o_3,\; o_1 \circ T(o_2,o_1)\big) \;=\; T\big(o_3,\; o_2 \circ T(o_1,o_2)\big)
$$

*Explanation.* The left side transforms $o_3$ against "$o_1$ then adjusted-$o_2$"; the right side against "$o_2$ then adjusted-$o_1$." TP2 demands these be **equal** — that transforming $o_3$ gives the same result no matter which equivalent path of prior transformations it travels. This is the property that fails. It must hold for *all* triples and, by induction, all $n$-tuples of concurrent operations, and the number of distinct transformation paths a late operation must be consistent across grows **combinatorially**:

$$
\text{distinct transformation paths for the } n\text{-th concurrent op} = \Theta(n!) \text{ in the worst case}
$$

*Why this is the cost.* Designing a $T$ that provably satisfies TP2 for a real operation set is so hard that **multiple published, peer-reviewed OT algorithms were later proven incorrect** — the literature documents a recurring history of TP2 violations discovered years after publication. The pragmatic escape the industry took (Jupiter, Google Docs) is to **impose a total order via a central server**, so that operations are linearized and TP2's multi-path nightmare is sidestepped — but that reintroduces exactly the central authority that CAP (A.3.1) told us we cannot require for an offline-first tool. So OT's costs are twofold and both disqualifying for us:

$$
\textbf{Correctness cost:}\quad \text{TP2 is combinatorially fragile; provably-correct } T \text{ for rich ops is a research problem.}
$$
$$
\textbf{Architectural cost:}\quad \text{the practical fix (total-order server) violates partition-tolerant availability.}
$$

CRDTs dominate modern systems precisely because they obtain convergence from *commutativity* (B.2.1) — a local, pairwise, checkable property — instead of from a global, multi-path, server-mediated transformation invariant. This is the rigorous version of "we don't build on OT."

## B.6 The rejected lineage II: patch algebra and category theory — beautiful, and why we only borrow it

The patch-theory lineage (Darcs, Pijul) is the most mathematically elegant and is built around exactly our prized operation: applying an individual change out of order. We borrow its *philosophy* and decline its *engine*; here is the mathematics that explains both halves of that decision.

### B.6.1 Categories, in one paragraph of definitions

A **category** $\mathcal{C}$ consists of objects and **morphisms** (arrows) $f: A \to B$ between them, with an associative composition ($h \circ (g \circ f) = (h \circ g) \circ f$) and an identity morphism $\mathrm{id}_A: A \to A$ at each object. *For version control:* objects are **document states** (files), and a morphism $f: A \to B$ is a **patch** taking state $A$ to state $B$. Composition is "apply one patch then another."

### B.6.2 Merge as a pushout (the commutative diagram)

Two collaborators diverge from a common ancestor $O$: patches $f: O \to A$ (Alice) and $g: O \to B$ (Bob). The merge is the **pushout** — an object $D$ with patches $i: A \to D$, $j: B \to D$ making the square commute and universal:

```
            f
       O ───────► A
       │          │
     g │          │ i          i ∘ f  =  j ∘ g       (the square commutes)
       ▼          ▼
       B ───────► D            and D is universal: for any other D' completing
            j                  the square, there is a unique  u : D → D'
```

Formally the **universal property**:

$$
i \circ f = j \circ g, \quad\text{and}\quad \forall\, (D', i', j') \text{ with } i'\circ f = j'\circ g,\;\; \exists!\, u: D \to D'.\; u\circ i = i' \wedge u\circ j = j'
$$

*Explanation.* "Commutes" ($i \circ f = j \circ g$): going $O \to A \to D$ equals going $O \to B \to D$ — both incorporate Alice's and Bob's changes to the same result $D$. "Universal" ($\exists!\,u$): $D$ is the *most general* such merge — any other valid merge $D'$ is a specialization of $D$ via a unique map. **This is the precise mathematical meaning of "the merge that includes both changes and invents nothing extra"** — and notice it is the categorical twin of the least-upper-bound (A.1.2): a pushout is a colimit, the categorical generalization of a join. The two rejected/adopted lineages and our chosen one all circle the same idea: *merge = most-general combination.*

### B.6.3 Why conflicts are missing pushouts, and Darcs' exponential blowup

Not every pair of patches has a pushout in the naive category $\mathcal{L}$ of files-and-patches: if Alice and Bob edit the *same line* incompatibly, **no object completes the square** — that absence *is* the conflict. Darcs handled conflicts with ad-hoc "conflictors/mergers," and in pathological histories its commutation-based merge could take **exponential time**:

$$
\text{Darcs worst-case merge} = O(2^{n}) \text{ in the number of interacting patches ("conflict fights")}
$$

Pijul's fix, from Mimram & Di Giusto's *A Categorical Theory of Patches* (2013), is to **freely complete** $\mathcal{L}$ under all pushouts — formally embedding it in a larger category $\mathcal{P}$ where every pair of patches *does* have a pushout, by representing files as graphs of byte-chunks with order-edges. Conflicts become first-class *states* (objects in $\mathcal{P}$) rather than exceptional failures, and merge becomes polynomial.

### B.6.4 What we take and what we leave

*Take (the philosophy):* model changes as **first-class, independently-applicable objects** that **commute when independent**, so that "accept this one suggestion, reject that one, in any order" is mathematically sound — the cherry-pick-by-commutation idea. This is *exactly* the property our review layer needs, and it is the categorical sibling of CmRDT commutativity (B.2.1).

*Leave (the engine):* a production pushout-completion engine is rare (Pijul is the lone mature instance and is young), the maintainer talent pool is minuscule, and operational debugging of free-cocompletion machinery is forbidding for a small team. So we **implement the philosophy with CmRDT/event-sourcing machinery** (commuting semantic operations on disjoint nodes, B.2.2 and Part D) — obtaining the cherry-pick property from a checkable commutativity argument we can actually staff and debug, rather than from category theory at runtime.

---

*Part B has built the data types and proved their behavior: CvRDT convergence from the semilattice laws (B.1.2); CmRDT convergence from commutativity (B.2.1); the sequence-CRDT machinery with the interleaving guarantee we require (B.3); the tombstone space cost and its GC condition (B.4); and the formal cost cases against OT (TP2's combinatorial fragility plus its server requirement, B.5) and for borrowing-not-building patch algebra (the pushout = most-general-merge, but an unstaffble engine, B.6). Part C turns to the tree algorithms — edit distance and the concurrent-move CRDT — and then to the systematic catalogue of expensive system-design patterns with the mathematics of each.*
---

# Part C — Tree algorithms, and the expensive patterns we reject

The questionnaire is a tree, and trees are where this class of system actually breaks. Part C gives the two relevant tree algorithms in full — edit distance (the expensive matching we *avoid*) and the concurrent-move CRDT (the hard primitive we *adopt*) — then catalogues the expensive system-design patterns with the mathematics of each cost, which is the part the design narrative skipped.

## C.1 Tree edit distance: the expensive matching problem we design *around*

### C.1.1 The problem

To three-way merge two trees against a common ancestor, you must first **match** nodes: decide which node in version $A$ corresponds to which in $B$ and in ancestor $O$. For unlabeled ordered trees with no stable identity, the principled way to do this is to compute the minimal **tree edit distance** — the cheapest sequence of relabel/insert/delete operations turning one tree into the other — and read the matching off the optimal script. This is the Zhang–Shasha algorithm (1989).

### C.1.2 The Zhang–Shasha recurrence

Let $T_1, T_2$ be ordered labeled trees. Operations have costs: $\gamma(\text{relabel } a \to b)$, $\gamma(a \to \varepsilon)$ (delete), $\gamma(\varepsilon \to b)$ (insert). Define $\mathrm{forestdist}(F_1, F_2)$ as the edit distance between two ordered forests. The recurrence, on the rightmost roots:

$$
\mathrm{forestdist}(\varnothing, \varnothing) = 0
$$
$$
\mathrm{forestdist}(F_1, \varnothing) = \mathrm{forestdist}(F_1 - v, \varnothing) + \gamma(v \to \varepsilon)
$$
$$
\mathrm{forestdist}(\varnothing, F_2) = \mathrm{forestdist}(\varnothing, F_2 - w) + \gamma(\varepsilon \to w)
$$
$$
\mathrm{forestdist}(F_1, F_2) = \min
\begin{cases}
\mathrm{forestdist}(F_1 - v,\; F_2) + \gamma(v \to \varepsilon) & \text{(delete } v) \\[4pt]
\mathrm{forestdist}(F_1,\; F_2 - w) + \gamma(\varepsilon \to w) & \text{(insert } w) \\[4pt]
\mathrm{forestdist}(F_1 - T(v),\, F_2 - T(w)) + \mathrm{treedist}(v, w) & \text{(match } v,w)
\end{cases}
$$

*Explanation, line by line.* $v$ and $w$ are the rightmost roots of forests $F_1, F_2$. The empty cases say turning a forest into nothing costs the sum of deletions, and building one from nothing costs the sum of insertions. The main recurrence picks the cheapest of three moves: **delete** the rightmost root of $F_1$ (and recurse on the rest), **insert** the rightmost root of $F_2$, or **match** the two rightmost subtrees $T(v), T(w)$ — paying their internal $\mathrm{treedist}$ and recursing on what remains. The optimal matching falls out of which "match" choices the minimum selected. It is the tree analog of the Levenshtein string-edit recurrence, with subtree structure replacing linear order.

### C.1.3 The cost — and why we refuse to pay it

The classic complexity is

$$
O\big(|T_1|\,|T_2|\,\min(\mathrm{depth}_1,\mathrm{leaves}_1)\,\min(\mathrm{depth}_2,\mathrm{leaves}_2)\big),
$$

i.e. roughly $O(n^4)$ for balanced trees of size $n$ (later algorithms — Klein; Demaine et al. — improve the worst case to $O(n^3)$ with worse constants). And crucially, even after paying this, the matching is a **heuristic guess** at correspondence: minimal-edit matching can pair up nodes that are not "the same" in any meaningful sense (this is why source-code AST-diff tools like GumTree layer further heuristics on top).

**Our design eliminates this entire cost.** Because every node carries a stable ULID identity (assigned at creation, invariant under move/edit/rename), correspondence across $O, A, B$ is **identity equality**:

$$
\text{match}(x, y) \iff \mathrm{id}(x) = \mathrm{id}(y), \qquad \text{cost } O(1) \text{ per node, } O(n) \text{ total, and \emph{exact}}
$$

*Explanation and the lesson.* We turned an $O(n^3$–$n^4)$ heuristic problem into an $O(n)$ exact lookup by *making identity a first-class property of the data model* (Part D, §D.1). This is the single highest-leverage decision in the whole system: the most expensive classical tree algorithm simply does not run, because we never have to *guess* what corresponds to what. Tree edit distance appears in this document only to be **explicitly rejected** as an avoidable expense — a paradigm of the Part C theme that *the right data-model decision deletes an entire algorithmic cost class.*

## C.2 The concurrent-move problem and its CRDT solution

Matching is free given identities, and per-field merge is straightforward (Part D). The one genuinely hard tree primitive that remains is **moving a subtree under concurrency**.

### C.2.1 Why move is hard: the cycle anomaly

Represent the tree as a child→parent map. A move sets a node's parent. Concurrency produces a failure with no analog in text:

```
   Ancestor O:        A          Replica X moves B under A's child A:   A
                     / \         (B → child of A)                      / \
                    A   B                                             A'  …
                                                                      |
   Replica Y concurrently moves A under B:    B          Merge naively →  A → B → A → …   CYCLE
                                              |           (X says B is under A; Y says A is under B)
                                              A           Result is a cycle, not a tree.
```

*Explanation.* $X$ makes $B$ a descendant of $A$; concurrently $Y$ makes $A$ a descendant of $B$. Apply both naively and you get $A$ under $B$ under $A$ — a **cycle**, so the structure is no longer a tree. Kleppmann, Mulligan, Gomes & Beresford (2021) *demonstrated this exact bug class in Google Drive and Dropbox*: concurrent moves producing duplicated, lost, or cyclically-related subtrees. A questionnaire with a section cyclically contained in itself is meaningless; we must forbid this while still converging.

### C.2.2 The move-operation CRDT — full algorithm

The state is a set of applied move operations and the derived child→parent map. Each move is timestamped with a globally unique, causally-aware timestamp $t$ (Lamport + replica id, A.2.2), giving a total order. Operations are applied **in timestamp order**, with an **undo–do–redo** discipline for late arrivals and a **cycle check** before each application.

```
type Move      = { t: Timestamp, child: NodeId, newParent: NodeId, meta }
type LogEntry  = { move: Move, oldParent: NodeId | null }     // oldParent recorded for undo
type State     = { tree: Map<NodeId, {parent, meta}>, log: List<LogEntry> }   // log sorted by t

ancestor(tree, a, b):                 # is a an ancestor of b in tree?
    p = tree[b].parent
    while p ≠ null:
        if p == a: return true
        p = tree[p].parent
    return false

do_op(move m, tree):                  # apply one move, with cycle protection
    oldParent = tree[m.child]?.parent
    entry = LogEntry{ move: m, oldParent: oldParent }
    if m.child == m.newParent or ancestor(tree, m.child, m.newParent):
        return (entry, tree)          # WOULD CREATE A CYCLE → record but do NOT apply (no-op)
    tree[m.child] = { parent: m.newParent, meta: m.meta }
    return (entry, tree)

undo_op(LogEntry e, tree):            # invert one applied move
    if e.oldParent ≠ null:
        tree[e.move.child].parent = e.oldParent
    else:
        remove tree[e.move.child]
    return tree

apply(move m, State s):               # the public operation
    # 1. Pop all log entries with timestamp > m.t, undoing them (newest first)
    popped = []
    while s.log.last.move.t > m.t:
        e = s.log.pop();  s.tree = undo_op(e, s.tree);  popped.push(e)
    # 2. Do m
    (entry, s.tree) = do_op(m, s.tree);  s.log.push(entry)
    # 3. Redo the popped ops in original (timestamp) order
    for e in reverse(popped):
        (e2, s.tree) = do_op(e.move, s.tree);  s.log.push(e2)
    return s
```

*Explanation of the three phases.* **(1) Undo:** when a move arrives "in the past" (its timestamp precedes some already-applied moves — the concurrent case), we roll the tree back by undoing those later moves, restoring the exact state that existed at $m$'s logical time. **(2) Do:** apply $m$, but only if it would not create a cycle (the `ancestor` guard); if it would, record it as applied but leave the tree unchanged — a deterministic no-op every replica performs identically. **(3) Redo:** replay the undone moves on top, in timestamp order. The net effect is that *every replica applies the entire set of moves in the same timestamp order*, so all reach the same tree.

### C.2.3 Why it converges (and stays a tree)

**Convergence.** After all moves propagate, every replica's log contains the same set of moves, and each replica's `apply` reorders its log into **timestamp order** via undo/redo. Since the timestamp order is a global total order (Lamport + replica-id tiebreak, identical everywhere), every replica ends with the identical sequence of `do_op` applications, hence identical `tree`. This is SEC (A.3.2) for trees.

**Tree invariant.** Each `do_op` runs the `ancestor` cycle check and refuses any move that would close a cycle. Since all replicas process the same moves in the same order, they make the *same* refusal decisions, so all converge to the same valid tree (acyclic, single-parent). Kleppmann et al. **machine-checked this in Isabelle/HOL** — the convergence and tree-validity proofs are formal, not informal, which is part of why we adopt this algorithm rather than improvise.

### C.2.4 Cost, and why it is cheap *for us*

Per `apply`, the undo/redo touches every log entry with a larger timestamp, and each `do_op`'s `ancestor` check is $O(\mathrm{depth})$:

$$
\text{cost of one move} = O\big(k \cdot \mathrm{depth}\big), \quad k = \#\{\text{concurrent later moves}\},\;\; \mathrm{depth} = \text{tree height}
$$

*Explanation.* In a huge filesystem with deep trees and many concurrent moves, $k$ and depth are large and this is a real cost (the paper benchmarks it). **Our trees are tiny** — tens to low-hundreds of nodes, depth 3–4 (form→section→item→choice), and moves are infrequent and rarely concurrent among a handful of editors. So $k$ and depth are both small constants and the algorithm is effectively free for us. Once again the *problem shape* (small expert-edited instrument, §A's whole motivation) converts a generally-hard primitive into a cheap one. We adopt the algorithm (directly, or via the Loro library, which implements exactly this plus fractional indexing for sibling order) precisely because its hard case is outside our regime.

## C.3 The expensive patterns — systematically, with the cost mathematics

The design narrative asserted that certain patterns "shouldn't be used" without showing the cost. Here is each rejected pattern, the formula for its cost, the failure it induces, and what we use instead. This is the catalogue the previous document skipped.

### C.3.1 Pessimistic locking (lock-the-document concurrency)

*Pattern.* Serialize edits by requiring a writer to acquire an exclusive lock before editing.

*Cost.* Throughput is bounded by lock round-trips; with edit duration $d$ and network RTT $r$, effective concurrency collapses to one writer per $\sim(d + 2r)$, and **offline editing is impossible** (you cannot acquire a lock with no network). Under CAP (A.3.1) this is choosing **C over A** — fatal for a field tool.

$$
\text{max edit throughput} = \frac{1}{d + 2r} \text{ edits/sec, single-writer} \quad\Rightarrow\quad \text{no concurrency, no offline}
$$

*Verdict.* Rejected outright; violates the availability requirement that CAP forced on us. We use optimistic replication (CRDT) instead — unlimited concurrent local edits, convergence on sync.

### C.3.2 Central-server total-order (OT-style) concurrency

*Pattern.* A server linearizes all operations to dodge TP2 (B.5.2).

*Cost.* Every edit incurs a server round-trip for ordering ($+r$ latency on the critical path), the server is a **single point of failure and a scaling bottleneck** ($O(\text{total edit rate})$ funneled through one ordering authority), and **offline edits cannot be ordered** until reconnection — again choosing C over A.

$$
\text{edit visible after } \ge r \text{ (server RTT)}; \quad \text{server load} = O\!\Big(\sum_{\text{clients}} \text{edit rate}\Big); \quad \text{offline} = \text{blocked}
$$

*Verdict.* Rejected as a *requirement*. A server is fine as an optional convenience/durability relay (it may run when online), but correctness must not depend on it (CRDT gives convergence with or without it). This is the precise sense in which we "use a server but don't require one."

### C.3.3 Pure state-based sync (ship the whole document)

*Pattern.* CvRDT shipping full state each sync (B.1.3).

*Cost.* $O(|s|)$ bandwidth per sync regardless of edit size; over $k$ edits, $O(k|s|)$ — bandwidth scales with **document size**, not change size, crippling on 2G field links.

*Verdict.* Rejected in favor of operation-based / delta-state sync, $O(\text{change})$ per edit.

### C.3.4 Naive event sourcing without snapshots

*Pattern.* Compute current state by folding the entire event log on every read (a tempting purity).

*Cost.* Read cost grows without bound:

$$
\text{cost to materialize current state} = O(N) \text{ in the number of events } N \text{ ever recorded}
$$

For a long-lived instrument with thousands of events this makes every "open the questionnaire" $O(N)$ — linear decay in responsiveness over the instrument's life.

*Fix we use (snapshotting).* Persist a materialized snapshot every $S$ events; a read folds only the tail since the last snapshot:

$$
\text{read cost with snapshots} = O(S) \text{ (constant in } N), \qquad \text{extra storage} = O(N/S \cdot |\text{snapshot}|)
$$

*Explanation.* Snapshot cadence $S$ trades read latency against storage: small $S$ → fast reads, more snapshot storage; large $S$ → slower reads, less storage. We tune $S$ so a read folds at most a few hundred events (sub-millisecond) and write a guaranteed snapshot at every `Deployed` event (so "the fielded version" is always $O(1)$ to retrieve). Naive snapshot-less event sourcing is the expensive pattern; snapshotted event sourcing is what we build.

### C.3.5 Storing the fat (uncompressed) CRDT state as the system of record

*Pattern.* Persist the CRDT's in-memory JSON (with every element's id and causal metadata) as the canonical store.

*Cost.* CRDT metadata inflates size by a factor $\phi$ over plain content; historically (early Automerge) $\phi$ reached an order of magnitude. Storing this fat form as the record multiplies all storage and transfer by $\phi$:

$$
\text{stored size} = \phi \cdot |\text{content}|, \quad \phi \gg 1 \text{ for naive encodings}
$$

*Fix we use.* Store the **columnar/binary** CRDT encoding (Automerge 2.0 / Yjs compact format) for live convergence, and the **semantic event log** (small, $\sim$150–400 B/event) as the system of record. Never persist the fat JSON.

### C.3.6 Three-way *text* merge on serialized structured documents (the git-on-JSON trap)

*Pattern.* Serialize the questionnaire to JSON/XLSForm and let git's line-based three-way merge reconcile versions (Architecture A of the design doc).

*Cost — two distinct failure modes:*

- **Spurious conflicts.** A moved block plus an edit inside it presents to a line-differ as overlapping line ranges → a conflict that does not semantically exist. The probability of a spurious conflict rises with serialization adjacency: edits to semantically-independent fields that happen to serialize to nearby lines collide. There is no clean closed form, but the qualitative cost is *false-positive conflicts proportional to serialization entanglement* — and every false conflict is a researcher-facing $<<<<<<<$ marker we promised never to show.
- **Silent semantic breakage (worse).** A line-clean merge can be semantically broken: if branch $A$ renames variable `hours_outdoors` and branch $B$ adds an item whose relevance references `${hours_outdoors}`, the textual merge succeeds but produces a **dangling reference** — a form that fails (perhaps silently) at deployment. Line merge has *no model of referential integrity*:

$$
\text{textual-merge correctness} \;\neq\; \text{semantic correctness}; \quad \text{the gap} = \{\text{dangling refs, orphaned choices, type/constraint incoherence, cycles}\}
$$

*Verdict.* Rejected as a merge engine. We keep git's *storage/audit* model (content-addressed, hash-linked history — cheap and valuable) but replace its merge with **semantic three-way merge over identified nodes** (Part D) plus a **referential-integrity pass** that catches exactly the silent-breakage class line merge cannot see. This is the formal statement of why "git-backed literal" is half-right (storage) and half-wrong (merge).

### C.3.7 Hand-rolling a sequence CRDT

*Pattern.* Implement your own RGA/text CRDT.

*Cost.* The no-interleaving property (B.3.4) and concurrent-insert convergence are subtle; the field's own history (multiple incorrect *published* algorithms) is the evidence. Expected cost is not just initial implementation but a **long correctness tail** — bugs that manifest only under specific concurrency, discovered in production, corrupting prompts.

*Verdict.* Rejected. Adopt a battle-tested library (Yjs/Automerge/Loro) for the convergence layer. "Never implement CRDTs from scratch" is the field's standing advice and we honor it; our novel work is *composition and the semantic/governance layers*, not the sequence-CRDT core.

### C.3.8 Summary table of rejected patterns

| Pattern | Dominant cost | Failure it causes | What we use instead |
|---|---|---|---|
| Pessimistic locking | $1/(d{+}2r)$ throughput; no offline | Unusable in the field (chooses C over A) | Optimistic CRDT replication |
| Central total-order (OT) | $+r$/edit; SPOF; no offline ordering | Server-bound; violates availability | CmRDT + optional relay |
| Pure state-based sync | $O(k\,|s|)$ bandwidth | Cripples low-bandwidth field links | Op-based / delta-state |
| Snapshot-less event sourcing | $O(N)$ per read | Linear responsiveness decay | Snapshotted fold, $O(S)$ |
| Fat CRDT-JSON as record | $\phi\,|content|$ storage | Order-of-magnitude bloat | Binary CRDT + semantic log |
| Git line-merge on JSON | spurious conflicts + silent ref-breakage | Researcher-facing markers; broken forms | Semantic merge + integrity pass |
| Hand-rolled sequence CRDT | long correctness tail | Prompt corruption under concurrency | Adopt Yjs/Automerge/Loro |

*The unifying principle of the catalogue:* **reject any pattern whose dominant cost scales with the wrong quantity** — document size instead of edit size, total history instead of recent tail, or that trades away the availability CAP forced us to keep. Every rejection above is an instance of that single rule, and every replacement restores the correct scaling.

---

*Part C has given the two tree algorithms in full — edit distance (rejected, $O(n^3$–$n^4)$ and heuristic, eliminated to $O(n)$ exact by stable identity, §C.1) and the concurrent-move CRDT (adopted, formally verified, cheap on our small trees, §C.2) — and the systematic cost catalogue of rejected patterns (§C.3). Part D assembles these into the formal system specification: the data model, the proof that our semantic operations commute on disjoint nodes (making the event log a CmRDT), the per-operation complexity table, and the storage equations.*
---

# Part D — The system, formally specified

Parts A–C built and costed the primitives. Part D assembles them into the recommended system with the formal properties stated and argued: the data model, the **commutativity theorem** that makes our semantic event log a CmRDT (the deepest justification for the whole architecture), the per-operation complexity table, the storage-growth equations, and the layered consistency argument.

## D.1 The data model, formally

The instrument is a typed, ordered, identified tree. Formally, a labeled ordered forest with stable identities:

$$
\mathcal{N} = \text{set of nodes}, \quad \mathrm{id}: \mathcal{N} \to \text{ULID (128-bit, invariant for node lifetime)}
$$
$$
\mathrm{parent}: \mathcal{N} \rightharpoonup \mathcal{N}, \quad \mathrm{pos}: \mathcal{N} \to \mathcal{P} \text{ (dense order key, B.3.2)}, \quad \mathrm{type}: \mathcal{N} \to \{\text{Form, Section, Item, Choice}\}
$$
$$
\text{fields}(n) = \text{a record of typed fields (prompt, varName, constraint, relevance, …) per } \mathrm{type}(n)
$$

subject to the **well-formedness invariants**:

$$
\textbf{(acyclic)}\quad \mathrm{parent}^{*} \text{ has no cycles} \qquad \textbf{(typed containment)}\quad \mathrm{Form} \ni \mathrm{Section} \ni \mathrm{Item} \ni \mathrm{Choice}
$$
$$
\textbf{(referential)}\quad \forall\, \text{expr } e \text{ in any field},\; \mathrm{refs}(e) \subseteq \{\mathrm{varName}(n) : n \in \mathcal{N}\} \;\wedge\; \mathrm{refgraph} \text{ acyclic}
$$

*Explanation.* The first invariant is the tree property the move CRDT (C.2) maintains. The second is the questionnaire's grammar (a choice cannot contain a section). The third is the referential-integrity contract (C.3.6's silent-breakage class made into an enforced invariant): every variable referenced in a relevance/constraint/calculation must exist, and the reference graph must be acyclic (no item's visibility may depend, transitively, on itself). **Identity being a ULID independent of position, content, and varName is the load-bearing decision** — it is what made matching $O(n)$ exact (C.1.3) and what makes the commutativity proof below clean.

## D.2 The operation set and the commutativity theorem

Our semantic operations (the events) are listed in the design document; abstractly each is a function $o: \Sigma \to \Sigma$ on instrument states, tagged with a target node id $\mathrm{tgt}(o)$ and a target field $\mathrm{fld}(o)$ (for content ops). The central claim:

**Theorem (disjoint-node commutativity).** For any two operations $o_1, o_2$ with $\mathrm{tgt}(o_1) \ne \mathrm{tgt}(o_2)$, or with $\mathrm{tgt}(o_1) = \mathrm{tgt}(o_2)$ but $\mathrm{fld}(o_1) \ne \mathrm{fld}(o_2)$,

$$
o_1 \circ o_2 = o_2 \circ o_1.
$$

*Proof.* Each content operation reads and writes only the single field $(\mathrm{tgt}(o), \mathrm{fld}(o))$. If the targets differ, or the targets agree but the fields differ, then $o_1$ and $o_2$ touch **disjoint locations** of the state record. Writes to disjoint memory commute: the value left at $o_1$'s location is the same whether $o_2$ ran before or after, and vice versa, and neither reads the other's location. Hence $o_1 \circ o_2$ and $o_2 \circ o_1$ produce identical states. Structural operations (`ItemAdded`, `ChoiceAdded`) on distinct fresh ULIDs likewise write disjoint new locations and commute; an addition and a content-edit to a *different* node commute for the same disjointness reason. $\blacksquare$

*Explanation and why this is the keystone.* Recall (B.2.1) that an operation-based CRDT converges iff **concurrent operations commute** under causal delivery. The theorem says **all our operations on disjoint (node, field) locations commute** — which is the overwhelming majority of real concurrent editing (two researchers working on different items, or on different fields of the same item). Therefore, restricted to disjoint locations, **our semantic event log *is* a CmRDT**: attach vector clocks (A.2.3) for causal delivery, and convergence is guaranteed by this theorem with no separate CRDT machinery. This answers the design document's central open question (its §X.3) in the affirmative *for the disjoint case.*

**The non-commuting residue, characterized exactly.** The proof's hypotheses fail in precisely one situation: $\mathrm{tgt}(o_1) = \mathrm{tgt}(o_2)$ **and** $\mathrm{fld}(o_1) = \mathrm{fld}(o_2)$ — two concurrent writes to the *same field of the same node* (e.g. two different rewordings of one prompt, or two different new relevance expressions for one item). For these, generally $o_1 \circ o_2 \ne o_2 \circ o_1$, so they are *not* CmRDT-convergent and must be handled by one of:

$$
\textbf{(a) RichText fields:}\quad \text{embed a sequence CRDT (B.3) in the field} \Rightarrow \text{the two edits commute \emph{within} the text CRDT.}
$$
$$
\textbf{(b) register fields (type, constraint, relevance, varName):}\quad \text{surface a same-field conflict for human decision (four-card).}
$$

*Explanation — the design's deepest coherence.* The set of operation pairs we route to human adjudication is **exactly** the set that violates commutativity, and **no others.** This is not an arbitrary product choice; it is forced by the algebra. For prose fields we restore commutativity by nesting a text CRDT (so co-editing a prompt converges automatically). For high-consequence scalar fields we *deliberately decline* to impose an artificial commutative rule (like last-writer-wins), because any such rule would silently pick a winner for, say, two different skip-logic expressions — and silently mis-merging skip logic corrupts a study. So we surface it. **The architecture thus partitions all concurrency into "provably commutes → converge automatically" and "provably doesn't → ask a human," with the boundary drawn by the commutativity theorem itself.** That is the rigorous core the narrative gestured at and could not state.

## D.3 The layered architecture and its consistency argument

```
┌──────────────────────────────────────────────────────────────────────────┐
│ L4  GOVERNANCE      roles · proposal lifecycle · approval policy           │
│                     operates on PROPOSED events; gates folding             │
│                     property: no event becomes canonical without policy    │
│                     satisfaction (capability checked AT THE FOLD)          │
├──────────────────────────────────────────────────────────────────────────┤
│ L3  AUDIT/HISTORY   append-only, content-addressed, hash-linked log of     │
│                     SEMANTIC events + periodic snapshots                   │
│                     property: tamper-evident (Merkle chain, A: git model); │
│                     read = O(S) via snapshots (C.3.4)                      │
├──────────────────────────────────────────────────────────────────────────┤
│ L2  CONVERGENCE     within a draft: CmRDT (disjoint ops, D.2 theorem) +    │
│                     embedded text CRDTs (RichText) + move CRDT (C.2) for   │
│                     reparenting + fractional index (B.3.5) for sibling     │
│                     order.  property: Strong Eventual Consistency (A.3.2)  │
├──────────────────────────────────────────────────────────────────────────┤
│ L1  MERGE ENGINE    between drafts only: semantic 3-way merge over         │
│  (deferred to V2)   identified nodes (O(n), C.1.3) + referential-integrity │
│                     pass (D.1 invariant) + four-card for same-field clash  │
└──────────────────────────────────────────────────────────────────────────┘
```

**Consistency argument, layer by layer.**

- **L2 within a draft is SEC.** Disjoint-node ops commute (D.2 theorem) ⇒ CmRDT convergence (B.2.1); RichText nests a no-interleaving sequence CRDT (B.3.4); reparenting uses the Isabelle-verified move CRDT (C.2.3); sibling order uses fractional indexing (B.3.5). Every sub-structure is independently SEC, and a product of SEC components, synchronized by the same causal channel, is SEC. Same-field register clashes are *escalated*, not silently merged (D.2(b)) — the one place we exchange automatic convergence for correctness, by design.
- **L3 is a tamper-evident, $O(S)$-readable record.** Content-addressing + hash-linking (git's model, A/B.6.2 colimit kinship) gives integrity; snapshots bound read cost (C.3.4). The log is the system of record; the CRDT state is a cache of it.
- **L4 never touches merge.** Governance gates *folding* of proposed events; it is a predicate over (event, policy, capabilities), evaluated at the fold, with every decision itself an event in L3. This realizes the concurrency/governance separation (the design doc's Part III) as a strict layering: L2 converges, L4 governs, and they share only the L3 log.
- **L1 runs only between drafts**, where human adjudication is expected, and is the sole component carrying the (small, on our trees) move-CRDT and semantic-merge cost; it is deferred to V2, so V1's consistency rests entirely on L2–L4.

## D.4 Per-operation complexity

With $n$ = nodes in the instrument (tens–hundreds), $S$ = snapshot cadence, $\mathrm{depth}$ = tree height (3–4), $L$ = length of an ordered list (tens):

| Operation | Complexity | Note |
|---|---|---|
| Local content edit (prompt/constraint/…) | $O(1)$ amortized | write one field + append one event |
| Insert item/choice (fractional index) | $O(1)$ typical, $O(L)$ on rebalance | B.3.5 |
| Reorder sibling | $O(1)$ (rewrite one key) | B.3.5 |
| Reparent (move) | $O(k\cdot\mathrm{depth})$, $k$=concurrent later moves | C.2.4; $k,\mathrm{depth}$ small ⇒ ~$O(1)$ |
| Materialize current state (read) | $O(S)$ | snapshot + tail fold, C.3.4 |
| Referential-integrity pass | $O(n + E)$, $E$=reference edges | parse expressions, check graph |
| Match nodes for 3-way merge | $O(n)$ exact | stable-id equality, C.1.3 (vs $O(n^3)$ tree-edit) |
| Semantic 3-way merge (L1) | $O(n + E)$ | per-node field merge + integrity pass |
| Detect concurrency of two edits | $O(\#\text{replicas})$ | vector-clock compare, A.2.3 |
| Sync one edit (delta/op) | $O(|\delta|) \approx O(\text{edit size})$ | B.1.3 / B.2 |

*Explanation.* Every hot-path operation is $O(1)$ or near it on our problem sizes; the only superlinear classical algorithm (tree-edit matching) is engineered away to $O(n)$ by stable identity; the only potentially-costly CRDT primitive (move) is cheap because our trees are small. **There is no operation whose cost scales with total history on the hot path** (reads are $O(S)$, not $O(N)$) — the unifying anti-pattern rule of C.3.8, satisfied throughout.

## D.5 Storage growth

Let $N$ = total events over the instrument's life, $\bar{e}$ = mean event size ($\approx$150–400 B), $S$ = snapshot cadence (events/snapshot), $|\mathrm{snap}|$ = snapshot size ($\approx$50–200 KB).

$$
\text{log storage} = N \cdot \bar{e}
$$
$$
\text{snapshot storage} = \frac{N}{S}\cdot |\mathrm{snap}|
$$
$$
\text{total} = N\Big(\bar{e} + \frac{|\mathrm{snap}|}{S}\Big)
$$

*Explanation and the cadence optimization.* The first term (log) is irreducible — it *is* the audit trail, and it is cheap ($N \cdot \bar e \approx$ a few MB even for thousands of edits, A.7 of the narrative). The second term (snapshots) is a knob: read latency is $O(S)$, snapshot storage is $O(1/S)$, so choosing $S$ trades the two. Minimizing total cost+latency gives an interior optimum; in practice we fix $S$ so a tail fold is sub-millisecond (a few hundred events) and force a snapshot at every `Deployed` event (so the fielded version is $O(1)$ to fetch and permanently pinned). At our scale total storage is single-digit GB/year per active institution — the audit-maximalist design is, as claimed, economically free (narrative Part VII), and here is the formula that shows it: both terms are linear in $N$ with tiny constants, and $N$ itself is small because instruments are small and finite-lived.

## D.6 What the rigor changed

Stating the mathematics did not merely decorate the earlier design; it *altered and sharpened* it in three concrete ways worth surfacing:

1. **The human-adjudication boundary is now derived, not chosen.** D.2 proves that the exact set of operations we route to the four-card decision is the complement of the commuting set. The product behavior ("surface same-field scalar clashes, auto-merge everything else") is *forced* by the commutativity theorem — a far stronger justification than "it seemed safer."

2. **The event log is shown to *be* a CmRDT on the disjoint case**, which means the design document's biggest open question — can the log double as the convergence engine and let us drop a separate CRDT dependency for the common case — is answered yes for disjoint-node edits, with the residue (same-field clashes) handled by nested text CRDTs and four-card. This is a genuine architectural simplification that only the proof revealed.

3. **Every rejected pattern now has a cost formula** (C.3) rather than a verdict, so the choices are auditable by a skeptical systems reviewer: locking and central-OT are rejected by CAP-forced availability; pure-state and snapshot-less and fat-JSON by wrong-quantity scaling; git-line-merge by the textual-vs-semantic correctness gap; hand-rolled CRDTs by the correctness tail. The unifying rule — *reject any pattern whose dominant cost scales with the wrong quantity, or that trades away availability* — is now stated and applied uniformly.

The design stands where it did, but it now stands on proofs and cost formulas rather than on names of theorems. That was the gap, and this companion closes it.

---
---

# Part E — Merkle structures: the mathematics of the audit guarantee

Parts A–D earned their guarantees with proofs. The **audit and integrity** tier — the property that makes the change record defensible for an IRB amendment or a methods appendix — was, by contrast, only *named*: the design narrative called the history a "Merkle DAG" and asserted it was "tamper-evident," and Part D referred to the log as "content-addressed" and "hash-linked" without deriving what those words buy. That is an inconsistency, because the entire institutional-tier value proposition rests on this structure. Part E closes it with the same rigor applied everywhere else: the cryptographic hash assumptions, the chain and tree constructions, the tamper-evidence theorem with proof, the $O(\log n)$ inclusion proof with a worked example, and the content-addressing/deduplication mathematics that bounds the storage of Part D.5.

## E.1 The primitive: cryptographic hash functions

A **cryptographic hash function** $H : \{0,1\}^{*} \to \{0,1\}^{k}$ maps an arbitrary-length bitstring to a fixed $k$-bit digest ($k = 256$ for SHA-256, the function git is migrating to). Everything downstream rests on three assumed properties, stated precisely because the tamper-evidence proof invokes them by name:

$$
\textbf{(preimage resistance)}\quad \text{given } y,\; \text{finding any } x \text{ with } H(x)=y \text{ costs } \approx 2^{k} \text{ work}
$$
$$
\textbf{(second-preimage resistance)}\quad \text{given } x_1,\; \text{finding } x_2 \ne x_1 \text{ with } H(x_2)=H(x_1) \text{ costs } \approx 2^{k}
$$
$$
\textbf{(collision resistance)}\quad \text{finding any } x_1 \ne x_2 \text{ with } H(x_1)=H(x_2) \text{ costs } \approx 2^{k/2} \text{ (birthday bound)}
$$

*Explanation.* Preimage resistance says you cannot run the hash *backwards* — given a digest you cannot manufacture content producing it. Second-preimage resistance says given a *specific* document you cannot find a *different* document with the same digest — this is the one tamper-evidence leans on most, because an attacker is not free to choose both documents; they are stuck with the real history $x_1$ and must forge a substitute $x_2$ matching it. Collision resistance is weaker work ($2^{k/2}$ by the birthday paradox, hence $k=256$ to leave a $2^{128}$ margin) and concerns an attacker free to choose *both* sides. The numbers are the point: at $k=256$ these costs ($2^{256}, 2^{128}$) are physically unreachable, so "tamper-evident" will mean "forging an undetected change is as hard as breaking SHA-256," not merely "hard in practice." This reduction — *audit integrity reduces to a standard cryptographic assumption* — is exactly the kind of defensible statement a regulated research context wants, and the reason to do this part properly.

## E.2 The hash chain: the append-only log's integrity

The audit log of Part D.3 is a sequence of semantic events $e_1, e_2, \dots, e_N$. We make it tamper-evident by chaining: each entry commits to its content *and to the digest of its predecessor*:

$$
h_0 = H(\text{genesis}), \qquad h_i = H\big(\,\mathrm{content}(e_i) \;\Vert\; h_{i-1}\,\big) \quad\text{for } i \ge 1
$$

where $\Vert$ is concatenation. The single value $h_N$ — the **head** — is a commitment to the *entire* history.

*Explanation.* Each link folds the previous digest into its own, so $h_i$ depends on $e_i$ and, transitively through $h_{i-1}$, on every earlier event. This is precisely git's commit-parent structure: a commit hashes its tree plus its parent commit's hash, making the commit id a commitment to all ancestry. The head $h_N$ is therefore a 256-bit fingerprint of the whole log — publish it (in a lab notebook, an IRB submission, a timestamping service) and you have pinned the entire history at that moment.

### E.2.1 Tamper-evidence theorem (chain form)

**Theorem.** Suppose an adversary alters event $e_j$ to $e_j' \ne e_j$ (edit, insert, delete, or reorder at position $j$) while wishing the published head $h_N$ to remain valid. Then either the recomputed head differs from $h_N$ (the tamper is *detected* by recomputation), or the adversary has found a second preimage of $H$.

*Proof.* Recompute forward from $j$. The altered entry yields $h_j' = H(\mathrm{content}(e_j') \Vert h_{j-1})$. For the published head to survive, we need the chain to re-converge — at minimum $h_j' = h_j$ (so that $h_{j+1}, \dots, h_N$ are unchanged). But $h_j = H(\mathrm{content}(e_j)\Vert h_{j-1})$ with $e_j \ne e_j'$, so $h_j' = h_j$ means
$$
H\big(\mathrm{content}(e_j')\Vert h_{j-1}\big) = H\big(\mathrm{content}(e_j)\Vert h_{j-1}\big), \quad \text{inputs differ},
$$
which is exactly a **second preimage** of $H$ — costing $\approx 2^{k}$ (E.1). If instead the adversary lets $h_j'$ differ and recomputes $h_{j+1}', \dots, h_N'$ honestly, then $h_N' \ne h_N$ (again unless a collision is engineered somewhere along the way), so the published head no longer matches and the tamper is detected by anyone holding $h_N$. $\blacksquare$

*Explanation and the consequence we wanted.* Detection does not require scanning the log or trusting the storage provider: **comparing one 256-bit head against a previously-published head certifies the entire history.** Any insertion, deletion, reordering, or content edit *anywhere* either changes the head or breaks the hash function. This is the formal content of "tamper-evident," and it is why the audit tier can store the log on infrastructure it does not fully trust (a cloud bucket, a shared institutional server) and still make integrity claims: trust reduces to the head, and the head reduces to SHA-256. Note the honest scope — Merkle structure gives *tamper-evidence* (you can *detect* alteration), not *tamper-resistance* (it does not *prevent* someone with write access from rewriting the log and republishing a new head). Prevention needs an external anchor: a signature over the head (E.6) or publishing the head to an append-only third party. The design should therefore *sign* heads at decision and deployment points, which §E.6 addresses.

## E.3 The Merkle tree: logarithmic inclusion proofs

A hash chain certifies the *whole* head cheaply but proving that *one specific event* is included still seems to need the whole log. The **Merkle tree** (Merkle, 1979) fixes this and is the right structure for *snapshots* and for *inclusion proofs* to a thin offline client.

### E.3.1 Construction

Given leaves $d_1, \dots, d_n$ (for us: the events since a snapshot, or the nodes of a snapshot tree), build a binary tree of hashes:

$$
\text{leaf } \ell_i = H(\,0x00 \Vert d_i\,), \qquad
\text{internal } v = H(\,0x01 \Vert v_{\text{left}} \Vert v_{\text{right}}\,)
$$

up to a single **Merkle root** $r$.

*Explanation.* Leaves hash the data with a `0x00` domain-separation prefix; internal nodes hash the concatenation of their two children with a `0x01` prefix. The prefixes prevent a *second-preimage across levels* — without them an attacker could present an internal node's two-child concatenation as if it were a single leaf (the well-known Merkle-tree forgery); the distinct tags make leaf-hashes and node-hashes live in disjoint domains, closing that attack. The root $r$ is a commitment to all $n$ leaves, just as the chain head was — but now the *shape* is balanced, which is what makes the next result possible.

### E.3.2 The inclusion (authentication) proof

To prove leaf $d_i$ is in the tree with root $r$, reveal only the **authentication path**: the sibling hash at each level from the leaf up to the root — $\lceil \log_2 n \rceil$ hashes. The verifier recomputes:

$$
a_0 = H(0x00 \Vert d_i), \qquad
a_{t+1} = \begin{cases} H(0x01 \Vert a_t \Vert s_t) & \text{if } d_i \text{ is a left child at level } t \\ H(0x01 \Vert s_t \Vert a_t) & \text{if a right child}\end{cases}
$$

and accepts iff $a_{\lceil \log_2 n\rceil} = r$, where $s_t$ are the supplied sibling hashes.

$$
\textbf{proof size} = O(\log n) \text{ hashes}, \qquad \textbf{verification} = O(\log n) \text{ hash evaluations}
$$

*Explanation.* The verifier rebuilds the path from the claimed leaf to the root using only the siblings, never the rest of the tree. If the recomputed root matches the trusted root $r$, the leaf is proven included — and by collision resistance no *other* leaf value could have produced that same path to that same root. **Consequence for us:** a field device, or an auditor, holding only the 32-byte root can verify "event $e$ (e.g. the deployment of the fielded instrument) is genuinely part of this history" by receiving just $\log_2 n$ hashes — for a 4,000-event instrument, 12 hashes, ~384 bytes — instead of the whole multi-megabyte log. This is the structure that makes integrity checks viable over the same low-bandwidth, intermittently-connected links that motivated the CRDT choice in Part A. It is also exactly how Certificate Transparency logs and blockchain light clients prove inclusion, so it is battle-tested at far larger scale than ours.

### E.3.3 Worked example ($n=4$)

```
                      root  r = H(01 ‖ v12 ‖ v34)
                     /                      \
        v12 = H(01‖ℓ1‖ℓ2)            v34 = H(01‖ℓ3‖ℓ4)
         /          \                   /          \
   ℓ1=H(00‖d1)  ℓ2=H(00‖d2)      ℓ3=H(00‖d3)  ℓ4=H(00‖d4)
       d1           d2               d3           d4
```

*Proof that $d_3$ is included.* Supply the authentication path $\langle \ell_4,\; v_{12}\rangle$ (2 = $\log_2 4$ hashes). Verifier computes $a_0=\ell_3=H(00\Vert d_3)$, then $a_1=H(01\Vert a_0 \Vert \ell_4)=v_{34}$ (sibling on the right), then $a_2=H(01\Vert v_{12}\Vert a_1)=r$ (sibling on the left). If $a_2=r$, accept.

*Now tamper:* change $d_3 \to d_3'$. Then $\ell_3' \ne \ell_3$ (second-preimage resistance), so $v_{34}'\ne v_{34}$, so $r'\ne r$ — the change has propagated to the root, and any holder of the trusted $r$ rejects. *This is the picture that should accompany the audit tier:* one edited leaf reddens its entire path to the root. The diagram below renders exactly this.

## E.4 Tree versus DAG versus chain — which we use where

The earlier documents conflated three related shapes. They are distinct, with distinct uses:

- **Hash chain (linear)** — each entry commits to the previous (E.2). *Use:* the append-only **event log**, whose natural shape is a sequence. Cheap to append ($O(1)$: one hash), head certifies all history. Inclusion of an arbitrary past event is $O(N)$ to prove from the chain alone — which is why we *also* build trees over snapshot windows.
- **Merkle tree (balanced binary)** — leaves hashed up to a root (E.3). *Use:* **snapshots** (a Merkle root over the snapshot's nodes) and any context needing $O(\log n)$ inclusion proofs to a thin client. Append is $O(\log n)$ (rebuild one path); inclusion proof is $O(\log n)$.
- **Merkle DAG (content-addressed graph)** — nodes reference children by hash, with sharing (E.5). *Use:* the **instrument tree itself** when content-addressed, giving structural sharing across snapshots/versions. This is git's object model (blobs/trees/commits) precisely.

$$
\begin{array}{lccc}
& \textbf{append} & \textbf{whole-history commit} & \textbf{single-item inclusion proof} \\
\text{chain} & O(1) & h_N & O(N) \\
\text{tree} & O(\log n) & \text{root } r & O(\log n) \\
\text{DAG} & O(\text{depth}) & \text{root hash} & O(\text{depth}) + \text{sharing}
\end{array}
$$

*Explanation and our composition.* We use **all three, each where its costs win**: the *log* is a chain (append-dominated, $O(1)$); each *snapshot* carries a Merkle *tree* root for $\log n$ inclusion proofs; and the *instrument state* is a content-addressed *DAG* so that successive snapshots and Tier-2 alternative versions **share unchanged subtrees by hash** rather than re-storing them (E.5). The design narrative's loose "Merkle DAG" was right in spirit for the instrument model but wrong as a blanket term for the log; this taxonomy is the correction.

## E.5 Content-addressing and deduplication — the storage bound of D.5, derived

Make every node of the instrument tree addressed by the hash of its content (including the hashes of its children):

$$
\mathrm{addr}(v) = H\big(\mathrm{fields}(v) \;\Vert\; \mathrm{addr}(c_1)\Vert\cdots\Vert\mathrm{addr}(c_m)\big), \quad c_j = \text{children of } v
$$

Two consequences follow immediately and they are the ones Part D.5 assumed without proof:

**Deduplication.** Identical content has identical address, so it is stored once. If two snapshots (or the canonical draft and a Tier-2 alternative) share a subtree $T$ unchanged, both reference the same $\mathrm{addr}(T)$; the bytes of $T$ are not duplicated.

**Structural sharing / the cost of a new snapshot.** Editing one field of one node $v$ changes $\mathrm{addr}(v)$, hence the addresses of its ancestors up to the root, and *only* those — every sibling subtree keeps its address and is shared. So a snapshot taken after a single edit costs only the path from the edited node to the root:

$$
\text{incremental snapshot storage} = O(\mathrm{depth}) \text{ new nodes}, \quad \text{not } O(n)
$$

*Explanation.* This is the persistent-data-structure trick (the same one behind immutable maps and git's own efficiency): unchanged structure is shared by reference, changed structure is copied along one root-path. With our depth of 3–4, a snapshot after an edit allocates a handful of new node objects, not a copy of the whole instrument. This is the formal justification for D.5's claim that snapshot storage is cheap and that keeping many snapshots (one per deployment, plus periodic) is affordable — *most of each snapshot is shared with its predecessor by hash.* The earlier document asserted "deduplication" as a git inheritance; here it is derived from content-addressing and quantified at $O(\mathrm{depth})$ per snapshot.

## E.6 What Merkle structure does *not* give, and the signature layer

Honesty about scope, paralleling the tamper-evidence-vs-resistance caveat of E.2.1:

- Merkle structure gives **integrity relative to a trusted root**, not **authenticity of authorship**. It proves "this history hashes to $r$," not "Dr. Rahman authored event $e$." Authorship needs **digital signatures**: each governance decision (accept/reject) and each `Deployed` event is signed by its actor's key, $\sigma = \mathrm{Sign}_{sk}(h_i)$, verifiable with the public key. The hash chain then binds the signature to all prior history.
- Merkle structure gives **detection**, not **prevention** (E.2.1). Prevention of silent rewrite needs an external anchor for the head: a signed, timestamped head deposited with a party the institution trusts (an internal append-only audit service, or an external timestamping/transparency log). For the most regulated customers this is the difference between "we can show the record wasn't altered" and "the record could not have been altered without our knowing," and the design should make heads *signable and externally anchorable* even if V1 only signs locally.

$$
\textbf{integrity} \Leftarrow \text{Merkle root}; \qquad \textbf{authenticity} \Leftarrow \text{signatures over hashes}; \qquad \textbf{non-repudiation/prevention} \Leftarrow \text{externally anchored signed head}
$$

*Explanation.* The three properties stack: the Merkle root makes the history self-certifying against a trusted digest; signatures over the chained hashes attribute each link to an actor and make alteration of *attributed* history forge-resistant; anchoring the signed head externally removes the last gap (a custodian rewriting their own log and republishing). This is the full cryptographic basis for an audit trail that survives scrutiny in an IRB amendment or a data-management-plan review — and it is layered so V1 can ship with hashes-and-local-signing and add external anchoring only for the institutional tier that demands it.

## E.7 What Part E changed

Stating this rigorously did three things the loose treatment could not:

1. **The audit guarantee is now reduced to a named cryptographic assumption** (second-preimage / collision resistance of SHA-256), with a proof, rather than asserted as "tamper-evident." A skeptical reviewer can check the reduction.

2. **The three Merkle shapes are disentangled** (chain for the log, tree for snapshots/inclusion proofs, DAG for the content-addressed instrument), correcting the narrative's blanket "Merkle DAG," each placed where its cost profile wins (E.4 table).

3. **D.5's storage claims are derived, not assumed** — deduplication and the $O(\mathrm{depth})$ incremental-snapshot bound fall out of content-addressing (E.5), and the $O(\log n)$ inclusion proof (E.3.2) shows integrity verification is viable on the same thin, intermittent links that drove the CRDT choice — tying the audit tier back to the offline-first thesis of Part A.

The scope caveats (E.2.1, E.6) also sharpen the roadmap: Merkle structure is necessary but not sufficient for the strongest audit claims, and the signature/anchoring layer is the named addition the institutional tier will require.

## Appendix — equation index (where each is introduced)

- Partial-order axioms — A.1.1
- Join (lub) definition and the three semilattice laws — A.1.2
- Happened-before (L1–L3) and concurrency — A.2.1
- Lamport clock update + clock condition + its non-converse — A.2.2
- Vector clock update, comparison, and exact causality/concurrency — A.2.3
- CAP statement + partition proof sketch — A.3.1
- SEC three conditions — A.3.2
- CvRDT inflationary update + convergence theorem and proof — B.1.1–B.1.2
- State-based vs delta sync cost — B.1.3
- CmRDT commutativity convergence condition — B.2.1
- Dense-order "between" property — B.3.2
- RGA same-predecessor ordering — B.3.3
- Interleaving anomaly worked example — B.3.4
- Fractional-index midpoint + precision growth — B.3.5
- Tombstone space + causal-stability GC condition — B.4
- OT TP1 and TP2 equations + factorial path growth — B.5.1–B.5.2
- Pushout universal property + diagram + Darcs $O(2^n)$ — B.6.2–B.6.3
- Zhang–Shasha forest recurrence + $O(n^3$–$n^4)$ + $O(n)$ id-match — C.1.2–C.1.3
- Move-CRDT full pseudocode + convergence + $O(k\cdot\mathrm{depth})$ — C.2.2–C.2.4
- Expensive-pattern cost formulas + summary table — C.3
- Disjoint-node commutativity theorem + proof — D.2
- Per-operation complexity table — D.4
- Storage-growth equations + cadence optimization — D.5
- Cryptographic hash assumptions (pre-image / 2nd-preimage / collision) — E.1
- Hash-chain construction + tamper-evidence theorem and proof — E.2
- Merkle tree construction + domain separation + O(log n) inclusion proof — E.3
- Chain vs tree vs DAG cost table — E.4
- Content-addressing, deduplication, O(depth) incremental snapshot — E.5
- Integrity / authenticity / prevention layering — E.6

*End of the rigorous companion. Read alongside `research_ide_versioning_protocol.md`: that document is the argument; this one is the proof, the cost accounting, and the algorithms in full.*
