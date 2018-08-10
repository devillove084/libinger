#![cfg_attr(not(test), allow(dead_code))]

extern crate libc;
extern crate ucontext;

use libc::ucontext_t;
use ucontext::Context;
use ucontext::getcontext;
use ucontext::makecontext;
use ucontext::setcontext;

fn main() {
	getcontext_donothing();
	getcontext_setcontext();
	getcontext_succeedatnothing();
	//getcontext_nested();
	makecontext_setcontext();
	context_moveinvariant();
	context_swapinvariant();
	getcontext_killswap();
}

#[cfg_attr(test, should_panic(expected = "done"))]
#[cfg_attr(test, test)]
fn getcontext_donothing() {
	let mut reached = false;
	getcontext(|_| reached = true, || unreachable!()).unwrap();
	assert!(reached);
	panic!("done");
}

#[cfg_attr(test, should_panic(expected = "done"))]
#[cfg_attr(test, test)]
fn getcontext_setcontext() {
	let mut reached = false;
	getcontext(
		|context| {
			setcontext(context);
			unreachable!();
		},
		|| reached = true,
	).unwrap();
	assert!(reached);
	panic!("done");
}

#[cfg_attr(test, should_panic(expected = "done"))]
#[cfg_attr(test, test)]
fn getcontext_succeedatnothing() {
	let invalid = getcontext(|context| context, || unreachable!()).unwrap();
	assert!(setcontext(invalid).is_none());
	panic!("done");
}

#[cfg_attr(test, ignore)]
#[cfg_attr(test, should_panic(expected = "done"))]
#[cfg_attr(test, test)]
fn getcontext_nested() {
	use std::cell::Cell;

	let mut reached = true;
	let context = Cell::new(None);
	getcontext(
		|outer| getcontext(
			|inner| {
				context.set(Some(inner));
				setcontext(outer);
				unreachable!();
			},
			|| reached = true,
		).unwrap(),
		|| {
			setcontext(context.take().unwrap());
			unreachable!();
		},
	).unwrap();
	assert!(reached);
	panic!("done");
}

#[cfg_attr(test, should_panic(expected = "done"))]
#[cfg_attr(test, test)]
fn makecontext_setcontext() {
	use std::cell::Cell;
	use ucontext::MINSIGSTKSZ;

	thread_local! {
		static REACHED: Cell<bool> = Cell::new(false);
	}

	extern "C" fn call() {
		REACHED.with(|reached| reached.set(true));
	}

	let mut reached = false;
	getcontext(
		|mut successor| {
			let mut stack = [0u8; MINSIGSTKSZ];
			let predecessor = makecontext(call, &mut stack, Some(&mut successor)).unwrap();
			setcontext(predecessor);
			unreachable!();
		},
		|| reached = true,
	).unwrap();
	assert!(REACHED.with(|reached| reached.get()));
	assert!(reached);
	panic!("done");
}

fn ucontext(context: &mut Context) -> &mut ucontext_t {
	use std::mem::transmute;

	unsafe {
		transmute(context)
	}
}

fn uc_inbounds(within: *const ucontext_t, context: *const ucontext_t) -> bool {
	within > context && within < unsafe {
		context.add(1)
	}
}

#[cfg_attr(test, test)]
fn context_moveinvariant() {
	use ucontext::MoveInvariant;

	let mut context = getcontext(|context| context, || unreachable!()).unwrap();
	let context = ucontext(&mut context);
	context.after_move();
	assert!(uc_inbounds(context.uc_mcontext.fpregs as _, context));
}

#[cfg_attr(test, test)]
fn context_swapinvariant() {
	use ucontext::MoveInvariant;

	let mut first = getcontext(|context| context, || unreachable!()).unwrap();
	let mut second = getcontext(|context| context, || unreachable!()).unwrap();

	let second = ucontext(&mut second);
	{
		let first = ucontext(&mut first);
		first.after_move();
		second.after_move();
		first.uc_link = first.uc_mcontext.fpregs as _;
		second.uc_link = second.uc_mcontext.fpregs as _;
		assert!(uc_inbounds(first.uc_link, first));
		assert!(uc_inbounds(second.uc_link, second));
	}

	first.swap(second);
	let first = ucontext(&mut first);
	assert!(uc_inbounds(first.uc_link, second));
	assert!(uc_inbounds(second.uc_link, first));
	assert!(uc_inbounds(first.uc_mcontext.fpregs as _, first));
	assert!(uc_inbounds(second.uc_mcontext.fpregs as _, second));
}

#[cfg_attr(test, should_panic(expected = "done"))]
#[cfg_attr(test, test)]
fn getcontext_killswap() {
	use libc::SA_SIGINFO;
	use libc::SIGUSR1;
	use libc::pthread_kill;
	use libc::pthread_self;
	use libc::sigaction;
	use libc::siginfo_t;
	use std::cell::Cell;
	use std::io::Error;
	use std::mem::zeroed;
	use std::os::raw::c_int;
	use std::ptr::null_mut;

	thread_local! {
		static CONTEXT: Cell<Option<Context>> = Cell::new(None);
	}

	extern "C" fn handler(
		_: c_int,
		_: Option<&mut siginfo_t>,
		context: Option<&mut ucontext_t>,
	) {
		let context = context.unwrap();
		CONTEXT.with(|global| {
			let mut global = global.take().unwrap();
			global.swap(context);
		});
	}

	let config = sigaction {
		sa_flags: SA_SIGINFO,
		sa_sigaction: handler as _,
		sa_restorer: None,
		sa_mask: unsafe {
			zeroed()
		},
	};
	if unsafe {
		sigaction(SIGUSR1, &config, null_mut())
	} != 0 {
		panic!(Error::last_os_error());
	}

	let mut reached = false;
	getcontext(
		|context| {
			CONTEXT.with(|global| global.set(Some(context)));
			unsafe {
				pthread_kill(pthread_self(), SIGUSR1);
			}
			panic!(Error::last_os_error());
		},
		|| {
			reached = true;
		}
	).unwrap();
	assert!(reached);
	panic!("done");
}
