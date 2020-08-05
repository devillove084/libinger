// This module supports static function replacement with semantics that are roughly the opposite of
// the dynamic mechanism described in libgotcha_repl.h.  Each wrapper applies only within libgotcha
// itself, and is enforced for *neither* the rest of the program *nor* any statically-linked client
// library.  The latter behavior is possible because the static linker prefers to resolve external
// symbol references to a dynamic library rather than a static one when given the option.

#ifndef REPL_H_
#define REPL_H_

void repl_init(void);

#endif
