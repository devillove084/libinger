use id::Id;
use libc::ucontext_t;
use stable::StableMutAddr;
use std::cell::RefCell;
use std::io::Error;
use std::io::Result;
use std::ops::DerefMut;
use uninit::Uninit;
use void::Void;

pub struct Context<S: DerefMut<Target = [u8]>> {
	id: Id,
	context: RefCell<ucontext_t>,
	persistent: Option<Persistent<S>>,
}

pub struct HandlerContext (ucontext_t);

struct Persistent<S: DerefMut<Target = [u8]>> {
	stack: S,
	successor: Id,
}

pub fn getcontext<T, A: FnOnce(Context<Void>) -> T, B: FnMut() -> T>(scope: A, mut checkpoint: B) -> Result<T> {
	use libc::getcontext;
	use std::mem::forget;
	use volatile::VolBool;

	let mut unused = VolBool::new(true);
	let this = Context::default();
	let guard = this.id;
	if unsafe {
		getcontext(this.context.as_ptr())
	} != 0 {
		Err(Error::last_os_error())?;
	}

	let res;
	if unused.read() {
		unused.write(false);
		res = scope(this);
	} else {
		forget(this);
		forget(scope);
		res = checkpoint();
	}

	guard.invalidate();
	drop(checkpoint);

	Ok(res)
}

pub fn makecontext<S: DerefMut<Target = [u8]>, F: FnOnce(Context<S>)>(stack: S, gate: F, call: fn()) -> Result<()> {
	use std::mem::transmute;
	use std::os::raw::c_uint;

	extern "C" fn trampoline(lower: c_uint, upper: c_uint) {
		let gate = lower as usize | ((upper as usize) << 32);
		let gate: fn() = unsafe {
			transmute(gate)
		};
		gate();
	}

	let mut guard = None;
	getcontext(
		|successor| -> Result<()> {
			use libc::getcontext;
			use libc::makecontext;

			let mut this = Context::new(stack, successor.id);
			guard = Some(this.id);
			if unsafe {
				getcontext(this.context.as_ptr())
			} != 0 {
				Err(Error::last_os_error())?;
			}

			let call: usize = call as *const fn() as _;
			{
				let mut context = this.context.borrow_mut();
				let stack = &mut this.persistent.as_mut().unwrap().stack;
				context.uc_stack.ss_sp = stack.as_mut_ptr() as _;
				context.uc_stack.ss_size = stack.len();
				context.uc_link = successor.context.as_ptr();
				unsafe {
					makecontext(
						&mut *context,
						transmute(trampoline as extern "C" fn(c_uint, c_uint)),
						2,
						call,
						call >> 32
					);
				}
			}
			gate(this);

			Ok(())
		},
		|| Ok(()),
	)??;

	guard.take().expect("makecontext(): guard was already invalidated! (fell through to successor multiple times?)").invalidate();

	Ok(())
}

pub fn restorecontext<S: StableMutAddr<Target = [u8]>, F: FnOnce(Context<S>)>(persistent: Context<S>, scope: F) -> Result<()> {
	unimplemented!()
}

#[must_use]
pub fn setcontext<S: DerefMut<Target = [u8]>>(continuation: *const Context<S>) -> Option<Error> {
	use invar::MoveInvariant;
	use libc::setcontext;

	let continuation = unsafe {
		continuation.as_ref()
	}?;

	if ! continuation.id.is_valid() {
		None?;
	}
	continuation.id.invalidate_subsequent();
	debug_assert!(
		continuation.persistent.as_ref().map(|persistent|
			persistent.successor.is_valid()
		).unwrap_or(true),
		"setcontext(): makecontext()-generated Context is valid but has an invalid successor!"
	);

	continuation.context.borrow_mut().after_move();
	unsafe {
		setcontext(continuation.context.as_ptr());
	}

	Some(Error::last_os_error())
}

impl Context<Void> {
	fn default() -> Self {
		Self::from(None)
	}
}

impl<S: DerefMut<Target = [u8]>> Context<S> {
	fn new(stack: S, successor: Id) -> Self {
		Self::from(Some(Persistent {
			stack,
			successor,
		}))
	}

	fn from(persistent: Option<Persistent<S>>) -> Self {
		Self {
			id: Id::new(),
			context: RefCell::new(ucontext_t::uninit()),
			persistent,
		}
	}

	pub fn swap(&mut self, other: &mut HandlerContext) {
		unimplemented!();
	}
}

#[cfg(test)]
mod tests {
	use libc::ucontext_t;
	use super::getcontext;
	use ucontext::HandlerContext;

	#[test]
	fn context_moveinvariant() {
		use invar::MoveInvariant;

		let mut context = getcontext(|context| context, || unreachable!()).unwrap();
		let mut context = context.context.borrow_mut();
		assert!(! uc_inbounds(context.uc_mcontext.fpregs as _, &*context));
		context.after_move();
		assert!(uc_inbounds(context.uc_mcontext.fpregs as _, &*context));
	}

	#[test]
	fn context_swapinvariant() {
		use invar::MoveInvariant;
		use std::mem::transmute;

		let mut first = getcontext(|context| context, || unreachable!()).unwrap();
		let second = getcontext(|context| context, || unreachable!()).unwrap();
		let mut second = HandlerContext (second.context.into_inner());
		{
			let mut first = first.context.borrow_mut();
			let HandlerContext (second) = &mut second;
			assert!(! uc_inbounds(first.uc_mcontext.fpregs as _, &*first));
			assert!(! uc_inbounds(second.uc_mcontext.fpregs as _, second));

			first.after_move();
			second.after_move();
			first.uc_link = first.uc_mcontext.fpregs as _;
			second.uc_link = second.uc_mcontext.fpregs as _;
			assert!(uc_inbounds(first.uc_link, &*first));
			assert!(uc_inbounds(second.uc_link, second));
		}
		first.swap(&mut second);

		let first = first.context.borrow();
		let HandlerContext (second) = &mut second;
		assert!(uc_inbounds(first.uc_mcontext.fpregs as _, &*first));
		assert!(uc_inbounds(second.uc_mcontext.fpregs as _, second));
		assert!(uc_inbounds(first.uc_link, second));
		assert!(uc_inbounds(second.uc_link, &*first));
	}

	fn uc_inbounds(within: *const ucontext_t, context: *const ucontext_t) -> bool {
		within > context && within < unsafe {
			context.add(1)
		}
	}
}
