#ifndef ERROR_H_
#define ERROR_H_

enum error {
	SUCCESS = 0,
	ERROR_STATICALLY_LINKED,
	ERROR_RUNTIME_LOADED,
	ERROR_DLMOPEN,
	ERROR_FNAME_PATH,
	ERROR_OPEN,
	ERROR_MALLOC,
	ERROR_MMAP,
	ERROR_SIGACTION,
	ERROR_LIBASM,
	ERROR_DLADDR,
};

// Returns a statically-allocated string literal broadly describing the error, or NULL on SUCCESS.
const char *error_message(enum error);

// Returns a temporary string giving additional information, or NULL if none is available.
//
// Must be called before using any other function from any other library, with the exception of
// error_message().  The resulting string must be copied locally before using any other function
// from any other library: its contents are not guaranteed to be preserved.
const char *error_explanation(enum error);

#endif
