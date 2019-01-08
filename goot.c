#include "goot.h"

#include "handle.h"

#include <assert.h>

// Each Global Offset Override Table (GOOT) is associated with a particular Procedure Linkage
// Override Table (PLOT) codepage.  For each trampoline function entry in the latter, we store a
// pointer to the corresponding object file handle.  The PLOT stores a pointer to its GOOT, which
// its code uses to look up this handle.  The handle contains a record of its first entry within the
// GOOT, making it possible to translate a PLOT/GOOT index into a GOT one.  Thus, the PLOT code is
// able to look up the true address in the appropriate shadow GOT.
//
// The GOT itself holds the index of the first free entry (or -1 to indicate the table is full),
// followed by a series of entries, each of which is interpreted as free or allocated depending on
// whether its LSB is set or unset, respectively.  Free entries store the index of the "next" (read:
// a subsequent) such entry, whose meaning is as follows:
//   * Whenever there is no free entry after given one, its "next" field contains the sentinel -1.
//   * Otherwise, if its immediately following entry is also free, a given block's "next" field
//     contains the index of the *last* entry in this consecutive free block.
//   * Otherwise (i.e., if this is the last entry in a consecutive free block), its "next" field
//     contains the index of the next free entry after this one.
//
// The chief advantage of this scheme is that it makes the data structure easy to process from the
// assembly code invoked by the PLOT trampoline functions.  The price of this is these limitations:
//   * The entries corresponding to a given object file must be allocated contiguously;
//   * This results in increased fragmentation in the case of frequent runtime (un)loading, and
//   * It is impossible to accommodate libraries whose combined GOTs have more entries than a single
//     PLOT codepage; in practice, this forces each PLOT to comprise multiple contiguous pages.

void goot_init(struct goot *table) {
	table->first_free = 0;
	for(unsigned index = 0; index < PLOT_ENTRIES_PER_PAGE; ++index) {
		table->entries[index].free.odd_tag = 0x1;
		table->entries[index].free.next_free = PLOT_ENTRIES_PER_PAGE - 1;
	}
	table->entries[PLOT_ENTRIES_PER_PAGE - 1].free.next_free = -1u;
}

bool goot_insert_lib(struct goot *table, const struct handle *object) {
	unsigned start;
	unsigned prev = -1u;
	unsigned next = 0;
	unsigned entries = handle_got_num_entries(object);
	for(start = table->first_free; start != -1u && table->entries[start].free.odd_tag & 0x1;) {
		union goot_entry *entry = table->entries + start;
		assert(entry->free.odd_tag & 0x1);
		if((entry->free.next_free != -1u || entries == 1) &&
			entry->free.next_free - start + 1 >= entries) {
			entry += entries - 1;
			assert(entry->free.odd_tag & 0x1);
			if(entry->free.next_free == -1u || !(entry[1].free.odd_tag & 0x1))
				next = entry->free.next_free;
			else
				next = start + entries;
			entry[1 - (signed) entries].free.odd_tag = 0x0;
		} else if(entry->free.next_free != -1u) {
			prev = entry->free.next_free;
			entry = table->entries + prev;
			assert(entry->free.odd_tag & 0x1);
			start = entry->free.next_free;
		} else
			start = -1u;
	}
	if(start == -1u)
		return false;

	for(union goot_entry *entry = table->entries + start, *end = entry + entries;
		entry != end;
		++entry)
		entry->lib = object;
	if(prev == -1u)
		table->first_free = next;
	else
		for(union goot_entry *free = table->entries + prev; free->free.odd_tag & 0x1; ++free)
			free->free.next_free = next;

	return true;
}

bool goot_remove_lib(struct goot *table, unsigned first_index) {
	if(table->entries[first_index].free.odd_tag & 0x1)
		return false;

	const struct handle *object = table->entries[first_index].lib;
	unsigned entries = handle_got_num_entries(object);
	for(unsigned index = first_index; index < first_index + entries; ++index) {
		union goot_entry *entry = table->entries + index;
		assert(!(entry->free.odd_tag & 0x1));
		assert(entry->lib == object);
		entry->free.odd_tag = 0x1;
		entry->free.next_free = first_index + entries - 1;
	}

	unsigned next = table->first_free;
	unsigned prev = -1u;
	while(next < first_index) {
		union goot_entry *entry = table->entries + next;
		assert(entry->free.odd_tag & 0x1);
		if(prev == -1u || !(table->entries[prev + 1].free.odd_tag & 0x1))
			prev = next;
		next = entry->free.next_free;
	}
	table->entries[first_index + entries - 1].free.next_free = next;
	if(prev == -1u)
		table->first_free = first_index;
	else
		for(union goot_entry *free = table->entries + prev;
			free != table->entries + first_index; ++free)
			free->free.next_free = first_index + entries - 1;

	return true;
}
