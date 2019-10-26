

use ::argparse;
use ::crossbeam;
use ::libc;
use ::walkdir;

use crate::digests::*;
use crate::flags::*;
use crate::hashes::*;
use crate::prelude::*;
use crate::sinks::*;




pub fn main () -> (Result<(), io::Error>) {
	
	
	let mut _hashes_flags = HashesFlags {
			algorithm : &MD5,
		};
	
	let mut _format_flags = HashesFormatFlags {
			zero : false,
		};
	
	let mut _path = path::PathBuf::from ("");
	
	let mut _threads_count = 0 as usize;
	let mut _queue_size = 0 as usize;
	let mut _nice_level = 0 as i8;
	
	
	{
		let mut _parser = argparse::ArgumentParser::new ();
		_hashes_flags.argparse (&mut _parser);
		_format_flags.argparse (&mut _parser);
		_parser.refer (&mut _threads_count) .add_option (&["-w", "--workers-count"], argparse::Parse, "hashing workers count (16 by default)");
		_parser.refer (&mut _queue_size) .add_option (&["--workers-queue"], argparse::Parse, "hashing workers queue size (1024 times workers count by default)");
		_parser.refer (&mut _nice_level) .add_option (&["--nice"], argparse::Parse, "OS process scheduling priority (i.e. `nice`) (19 by default)");
		_parser.refer (&mut _path) .add_argument ("path", argparse::Parse, "starting file or folder") .required ();
		_parser.parse_args_or_exit ();
	}
	
	
	if _threads_count == 0 {
		_threads_count = 16;
	}
	if _queue_size == 0 {
		_queue_size = _threads_count * 1024;
	}
	if _nice_level == 0 {
		_nice_level = 19;
	}
	
	
	if _nice_level != 0 {
		unsafe {
			// FIXME:  Check the return value!
			libc::nice (_nice_level as i32);
		}
	}
	
	
	let _output = fs::OpenOptions::new () .write (true) .open ("/dev/stdout") ?;
	
	let _sink = StandardHashesSink::new (_output, _format_flags.zero);
	let _sink = sync::Arc::new (sync::Mutex::new (_sink));
	
	
	let mut _walker = walkdir::WalkDir::new (&_path)
			.follow_links (false)
			.same_file_system (false)
			.contents_first (true)
			.into_iter ();
	
	
	let (_enqueue, _dequeue) = crossbeam::channel::bounded::<walkdir::DirEntry> (_queue_size);
	let mut _completions = Vec::with_capacity (_threads_count);
	let _done = crossbeam::sync::WaitGroup::new ();
	
	
	for _ in 0 .. _threads_count {
		
		let _sink = sync::Arc::clone (&_sink);
		let _dequeue = _dequeue.clone ();
		let _done = _done.clone ();
		
		let _hashes_algorithm = _hashes_flags.algorithm;
		
		let _completion = thread::spawn (move || -> Result<(), io::Error> {
				
				let mut _hash_buffer = Vec::with_capacity (128);
				
				loop {
					
					let _entry = match _dequeue.recv () {
						Ok (_entry) =>
							_entry,
						Err (crossbeam::channel::RecvError) =>
							break,
					};
					
					let mut _file = fs::File::open (_entry.path ()) ?;
					
					_hash_buffer.clear ();
					digest (_hashes_algorithm, &mut _file, &mut _hash_buffer) ?;
					
					let mut _sink = _sink.lock () .unwrap ();
					_sink.handle (_entry.path () .as_os_str (), &_hash_buffer) ?;
				}
				
				drop (_done);
				
				return Ok (());
			});
		
		_completions.push (_completion);
	}
	
	
	loop {
		
		let _entry = if let Some (_entry) = _walker.next () {
			_entry ?
		} else {
			break;
		};
		
		let _metadata = _entry.metadata () ?;
		
		if _metadata.is_file () {
			_enqueue.send (_entry) .unwrap ();
		}
	}
	
	drop (_enqueue);
	drop (_dequeue);
	
	
	_done.wait ();
	
	for _completion in _completions.into_iter () {
		match _completion.join () {
			Ok (_outcome) =>
				_outcome ?,
			Err (_error) =>
				panic! (_error),
		}
	}
	
	return Ok (());
}

