// Copyright 2020 Konstantinos Gavalas.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A simple library for handling .srt subtitle files.
//!
//! This library allows you to handle subtitle files as collections of multiple subtitle structs,
//! letting you modify the subtitles without directly messing with the .srt files.
//!
//! Subtitle collections can be generated by parsing strings and files, but also from the ground
//! up, enabling total control of all the elements of each subtitle.
//!
//! # Examples
//! ```no_run
//! use srtlib::Subtitles;
//!
//! // Parse subtitles from file that uses the utf-8 encoding.
//! let mut subs = Subtitles::parse_from_file("subtitles.srt", None).unwrap();
//!
//! // Move every subtitle 10 seconds forward in time.
//! for s in &mut subs {
//!     s.add_seconds(10);
//! }
//!
//! // Write subtitles back to the same .srt file.
//! subs.write_to_file("subtitles.srt", None).unwrap();
//! ```
//!
//! ```no_run
//! use srtlib::{Timestamp, Subtitle, Subtitles};
//!
//! // Construct a new, empty Subtitles collection.
//! let mut subs = Subtitles::new();
//!
//! // Construct a new subtitle.
//! let one = Subtitle::new(1, Timestamp::new(0, 0, 0, 0), Timestamp::new(0, 0, 2, 0), "Hello world!".to_string());
//!
//! // Add subtitle at the end of the subs collection.
//! subs.push(one);
//!
//! // Construct a new subtitle by parsing a string.
//! let two = Subtitle::parse("2\n00:00:02,500 --> 00:00:05,000\nThis is a subtitle.".to_string()).unwrap();
//!
//! // Add subtitle at the end of the subs collection.
//! subs.push(two);
//!
//! // Write the subtitles to a .srt file.
//! subs.write_to_file("test.srt", None).unwrap();
//! ```
//!
//! ```
//! use std::fmt::Write;
//! use srtlib::Subtitles;
//!
//! # fn main() -> Result<(), srtlib::ParsingError> {
//! // Parse subtitles from a string and convert to vector.
//! let mut subs = Subtitles::parse_from_str("3\n00:00:05,000 --> 00:00:07,200\nFoobar\n\n\
//!                                           1\n00:00:00,000 --> 00:00:02,400\nHello\n\n\
//!                                           2\n00:00:03,000 --> 00:00:05,000\nWorld\n\n".to_string()
//!                                         )?.to_vec();
//!
//! // Sort the subtitles.
//! subs.sort();
//!
//! // Collect all subtitle text into a string.
//! let mut res = String::new();
//! for s in subs {
//!     write!(&mut res, "{}\n", s.text).unwrap();
//! }
//!
//! assert_eq!(res, "Hello\nWorld\nFoobar\n".to_string());
//!
//! # Ok(())
//! # }
//!
//! ```

use encoding_rs::*;
use std::fmt;
use std::fs;
use std::io::prelude::*;
use std::ops::Index;
use std::path::Path;

/// The error type returned by any function that parses strings or files.
#[derive(Debug)]
pub enum ParsingError {
    ParseIntError(std::num::ParseIntError),
    IOError(std::io::Error),
    MalformedTimestamp,
    BadSubtitleStructure(usize),
    BadEncodingName,
}

impl fmt::Display for ParsingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParsingError::ParseIntError(error) => write!(f, "{}", error),
            ParsingError::IOError(error) => write!(f, "{}", error),
            ParsingError::MalformedTimestamp => write!(f, "tried parsing a malformed timestamp"),
            ParsingError::BadEncodingName => write!(f, "incorrect encoding name provided; refer to https://encoding.spec.whatwg.org/#names-and-labels for available encodings"),
            ParsingError::BadSubtitleStructure(num) => {
                let number = if num > &0 { num.to_string() } else { String::from("unknown") }; 
                write!(f, "tried parsing an incorrectly formatted subtitle (subtitle number {})", number)
            }

        }
    }
}

impl std::error::Error for ParsingError {}

impl From<std::num::ParseIntError> for ParsingError {
    fn from(error: std::num::ParseIntError) -> Self {
        ParsingError::ParseIntError(error)
    }
}

impl From<std::io::Error> for ParsingError {
    fn from(error: std::io::Error) -> Self {
        ParsingError::IOError(error)
    }
}

/// A simple timestamp following the timecode format hours:minutes:seconds,milliseconds.
///
/// Used within the [`Subtitle`] struct to indicate the time that the subtitle should appear on
/// screen(start_time) and the time it should disappear(end_time).
/// The maximum value for any given Timestamp is 255:59:59,999.
///
/// # Examples
///
/// We can directly construct Timestamps from integers and they will always be displayed with the
/// correct timecode format:
/// ```
/// use srtlib::Timestamp;
///
/// let time = Timestamp::new(0, 0, 1, 200);
/// assert_eq!(time.to_string(), "00:00:01,200");
/// ```
///
/// We can also, for example, construct the Timestamp by parsing a string, move it forward in time by 65 seconds and then
/// print it in the correct format.
/// ```
/// use srtlib::Timestamp;
///
/// let mut time = Timestamp::parse("00:01:10,314").unwrap();
/// time.add_seconds(65);
/// assert_eq!(time.to_string(), "00:02:15,314");
/// ```
///
/// [`Subtitle`]: struct.Subtitle.html
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub struct Timestamp {
    hours: u8,
    minutes: u8,
    seconds: u8,
    milliseconds: u16,
}

impl Timestamp {
    /// Constructs a new Timestamp from integers.
    pub fn new(hours: u8, minutes: u8, seconds: u8, milliseconds: u16) -> Timestamp {
        Timestamp {
            hours,
            minutes,
            seconds,
            milliseconds,
        }
    }

    /// Constructs a new Timestamp by parsing a string with the format
    /// "hours:minutes:seconds,milliseconds".
    ///
    /// # Errors
    /// If this function encounters a string that does not follow the correct timecode format, a
    /// MalformedTimestamp error variant will be returned.
    pub fn parse(s: &str) -> Result<Timestamp, ParsingError> {
        let mut iter = s.splitn(3, ':');
        let hours = iter
            .next()
            .ok_or(ParsingError::MalformedTimestamp)?
            .parse()?;
        let minutes = iter
            .next()
            .ok_or(ParsingError::MalformedTimestamp)?
            .parse()?;
        let mut second_iter = iter
            .next()
            .ok_or(ParsingError::MalformedTimestamp)?
            .splitn(2, ',');
        let seconds = second_iter
            .next()
            .ok_or(ParsingError::MalformedTimestamp)?
            .parse()?;
        let milliseconds = second_iter
            .next()
            .ok_or(ParsingError::MalformedTimestamp)?
            .parse()?;

        Ok(Timestamp {
            hours,
            minutes,
            seconds,
            milliseconds,
        })
    }

    /// Moves the timestamp n hours forward in time.
    /// Negative values may be provided in order to move the timestamp back in time.
    ///
    /// # Panics
    ///
    /// Panics if we exceed the upper limit or go below zero.
    pub fn add_hours(&mut self, n: i32) {
        if n > (u8::MAX - self.hours) as i32 || -n > self.hours as i32 {
            panic!("Surpassed limits of Timestamp!");
        }
        self.hours = (self.hours as i32 + n) as u8;
    }

    /// Moves the timestamp n minutes forward in time.
    /// Negative values may be provided in order to move the timestamp back in time.
    ///
    /// # Panics
    ///
    /// Panics if we exceed the upper limit or go below zero.
    pub fn add_minutes(&mut self, n: i32) {
        let delta = (self.minutes as i32 + n) % 60;
        self.add_hours((self.minutes as i32 + n) / 60 - delta.is_negative() as i32);
        self.minutes = ((60 + delta) % 60).unsigned_abs() as u8;
    }

    /// Moves the timestamp n seconds forward in time.
    /// Negative values may be provided in order to move the timestamp back in time.
    ///
    /// # Panics
    ///
    /// Panics if we exceed the upper limit or go below zero.
    pub fn add_seconds(&mut self, n: i32) {
        let delta = (self.seconds as i32 + n) % 60;
        self.add_minutes((self.seconds as i32 + n) / 60 - delta.is_negative() as i32);
        self.seconds = ((60 + delta) % 60).unsigned_abs() as u8;
    }

    /// Moves the timestamp n milliseconds forward in time.
    /// Negative values may be provided in order to move the timestamp back in time.
    ///
    /// # Panics
    ///
    /// Panics if we exceed the upper limit or go below zero.
    pub fn add_milliseconds(&mut self, n: i32) {
        let delta = (self.milliseconds as i32 + n) % 1000;
        self.add_seconds((self.milliseconds as i32 + n) / 1000 - delta.is_negative() as i32);
        self.milliseconds = ((1000 + delta) % 1000).unsigned_abs() as u16;
    }

    /// Moves the timestamp forward in time by an amount specified as timestamp.
    ///
    /// # Panics
    ///
    /// Panics if we exceed the upper limit
    pub fn add(&mut self, timestamp: &Timestamp) {
        self.add_hours(timestamp.hours as i32);
        self.add_minutes(timestamp.minutes as i32);
        self.add_seconds(timestamp.seconds as i32);
        self.add_milliseconds(timestamp.milliseconds as i32);
    }

    /// Moves the timestamp backward in time by an amount specified as timestamp.
    ///
    /// # Panics
    ///
    /// Panics if we go below zero
    pub fn sub(&mut self, timestamp: &Timestamp) {
        self.add_milliseconds(-(timestamp.milliseconds as i32));
        self.add_seconds(-(timestamp.seconds as i32));
        self.add_minutes(-(timestamp.minutes as i32));
        self.add_hours(-(timestamp.hours as i32));
    }

    /// Returns the timestamp as a tuple of four integers (hours, minutes, seconds, milliseconds).
    pub fn get(&self) -> (u8, u8, u8, u16) {
        (self.hours, self.minutes, self.seconds, self.milliseconds)
    }

    /// Changes the timestamp according to the given integer values.
    pub fn set(&mut self, hours: u8, minutes: u8, seconds: u8, milliseconds: u16) {
        self.hours = hours;
        self.minutes = minutes;
        self.seconds = seconds;
        self.milliseconds = milliseconds;
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02}:{:02}:{:02},{:03}",
            self.hours, self.minutes, self.seconds, self.milliseconds
        )
    }
}

/// A single subtitle.
///
/// Contains the numeric counter, the beginning and end timestamps and the text of the subtitle.
///
/// # Examples
///
/// ```
/// use srtlib::Subtitle;
/// use srtlib::Timestamp;
///
/// let sub = Subtitle::new(1, Timestamp::new(0, 0, 0, 0), Timestamp::new(0, 0, 1, 0), "Hello world".to_string());
/// assert_eq!(sub.to_string(), "1\n00:00:00,000 --> 00:00:01,000\nHello world");
///
/// let sub = Subtitle::parse("2\n00:00:01,500 --> 00:00:02,500\nFooBar".to_string()).unwrap();
/// assert_eq!(sub.text, "FooBar");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Subtitle {
    pub num: usize,
    pub start_time: Timestamp,
    pub end_time: Timestamp,
    pub text: String,
}

impl Subtitle {
    /// Constructs a new Subtitle.
    pub fn new(num: usize, start_time: Timestamp, end_time: Timestamp, text: String) -> Subtitle {
        Subtitle {
            num,
            start_time,
            end_time,
            text,
        }
    }

    /// Construct a new subtitle by parsing a string with the format "num\nstart --> end\ntext" or the format
    /// "num\nstart --> end position_information\ntext" where start and end are timestamps using the format
    /// hours:minutes:seconds,milliseconds ; and position_information is position information of any format
    ///
    /// # Errors
    ///
    /// If this function encounters anything unexpected while parsing the string, a corresponding error variant
    /// will be returned.
    pub fn parse(input: String) -> Result<Subtitle, ParsingError> {
        let mut iter = input.trim_start_matches('\n').splitn(3, '\n');
        let num = iter
            .next()
            .ok_or(ParsingError::BadSubtitleStructure(0))?
            .parse::<usize>()?;
        let time = iter.next().ok_or(ParsingError::BadSubtitleStructure(num))?;
        let mut time_iter = time.split(" --> ");
        let start = Timestamp::parse(
            time_iter
                .next()
                .ok_or(ParsingError::BadSubtitleStructure(num))?,
        )?;
        let end_with_possible_position_info = time_iter
            .next()
            .ok_or(ParsingError::BadSubtitleStructure(num))?;
        let end = Timestamp::parse(
            end_with_possible_position_info
                .split(' ')
                .next()
                .ok_or(ParsingError::BadSubtitleStructure(num))?,
        )?;
        let text = iter.next().ok_or(ParsingError::BadSubtitleStructure(num))?;
        Ok(Subtitle::new(num, start, end, text.to_string()))
    }

    /// Moves the start and end timestamps n hours forward in time.
    /// Negative values may be provided in order to move the timestamps back in time.
    ///
    /// # Panics
    ///
    /// Panics if we exceed the upper limit or go below zero.
    pub fn add_hours(&mut self, n: i32) {
        self.start_time.add_hours(n);
        self.end_time.add_hours(n);
    }

    /// Moves the start and end timestamps n minutes forward in time.
    /// Negative values may be provided in order to move the timestamps back in time.
    ///
    /// # Panics
    ///
    /// Panics if we exceed the upper limit or go below zero.
    pub fn add_minutes(&mut self, n: i32) {
        self.start_time.add_minutes(n);
        self.end_time.add_minutes(n);
    }

    /// Moves the start and end timestamps n seconds forward in time.
    /// Negative values may be provided in order to move the timestamps back in time.
    ///
    /// # Panics
    ///
    /// Panics if we exceed the upper limit or go below zero.
    pub fn add_seconds(&mut self, n: i32) {
        self.start_time.add_seconds(n);
        self.end_time.add_seconds(n);
    }

    /// Moves the start and end timestamps n milliseconds forward in time.
    /// Negative values may be provided in order to move the timestamps back in time.
    ///
    /// # Panics
    ///
    /// Panics if we exceed the upper limit or go below zero.
    pub fn add_milliseconds(&mut self, n: i32) {
        self.start_time.add_milliseconds(n);
        self.end_time.add_milliseconds(n);
    }

    /// Moves the start and end timestamps forward in time by an amount specified as timestamp.
    ///
    /// # Panics
    ///     
    /// Panics if we exceed the upper limit
    pub fn add(&mut self, timestamp: &Timestamp) {
        self.start_time.add(timestamp);
        self.end_time.add(timestamp);
    }

    /// Moves the start and end timestamps backward in time by an amount specified as timestamp.
    ///
    /// # Panics
    ///
    /// Panics if we go below zero
    pub fn sub(&mut self, timestamp: &Timestamp) {
        self.start_time.sub(timestamp);
        self.end_time.sub(timestamp);
    }
}

impl fmt::Display for Subtitle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}\n{} --> {}\n{}",
            self.num, self.start_time, self.end_time, self.text
        )
    }
}

/// A collection of [`Subtitle`] structs.
///
/// Provides an easy way to represent an entire .srt subtitle file.
///
/// # Examples
///
/// ```
/// use srtlib::{Subtitle, Subtitles};
///
/// let mut subs = Subtitles::new();
/// subs.push(Subtitle::parse("1\n00:00:00,000 --> 00:00:01,000\nHello world!".to_string()).unwrap());
/// subs.push(Subtitle::parse("2\n00:00:01,200 --> 00:00:03,100\nThis is a subtitle!".to_string()).unwrap());
///
/// assert_eq!(subs.to_string(),
///            "1\n00:00:00,000 --> 00:00:01,000\nHello world!\n\n2\n00:00:01,200 --> 00:00:03,100\nThis is a subtitle!");
/// ```
///
/// [`Subtitle`]: struct.Subtitle.html
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Subtitles(Vec<Subtitle>);

impl Subtitles {
    /// Constructs a new(empty) Subtitles collection.
    pub fn new() -> Subtitles {
        Default::default()
    }

    /// Constructs a new Subtitles collection from a vector of [`Subtitle`] structs.
    ///
    /// [`Subtitle`]: struct.Subtitle.html
    pub fn new_from_vec(v: Vec<Subtitle>) -> Subtitles {
        Subtitles(v)
    }

    /// Constructs a new Subtitles collection by parsing a string with the format
    /// "subtitle\n\nsubtitle\n\n..." where subtitle is a string formatted as described in the
    /// [`Subtitle`] struct documentation.
    ///
    /// # Errors
    ///
    /// If this function encounters anything unexpected while parsing the string, a corresponding error variant
    /// will be returned.
    ///
    /// [`Subtitle`]: struct.Subtitle.html
    pub fn parse_from_str(mut input: String) -> Result<Subtitles, ParsingError> {
        let mut res = Subtitles::new();

        input = input.trim_start_matches('\u{feff}').to_string();
        if input.contains('\r') {
            input = input.replace('\r', "");
        }

        for s in input
            .split_terminator("\n\n")
            // only parse lines that include alphanumeric characters
            .filter(|&x| x.contains(char::is_alphanumeric))
        {
            res.push(Subtitle::parse(s.to_string())?);
        }

        Ok(res)
    }

    /// Constructs a new Subtitles collection by parsing a .srt file.
    ///
    /// **encoding** should either be Some("encoding-name") or None if using utf-8.
    /// For example if the file is using the ISO-8859-7 encoding (informally referred to as
    /// Latin/Greek) we could use:
    /// ```no_run
    /// use srtlib::Subtitles;
    /// # fn main() -> Result<(), srtlib::ParsingError> {
    /// let subs = Subtitles::parse_from_file("subtitles.srt", Some("iso-8859-7"))?;
    /// # Ok(())
    /// # }
    /// ```
    /// or the equivalent:
    /// ```no_run
    /// # use srtlib::Subtitles;
    /// # fn main() -> Result<(), srtlib::ParsingError> {
    /// let subs = Subtitles::parse_from_file("subtitles.srt", Some("greek"))?;
    /// # Ok(())
    /// # }
    /// ```
    /// For a list of encoding names (labels) refer to the [Encoding Standard].
    ///
    /// # Errors
    ///
    /// If the encoding label provided is not one of the labels specified by the [Encoding
    /// Standard], a BadEncodingName error
    /// variant will be returned.
    ///
    /// If something unexpected is encountered during the parsing of the contents of the file, a
    /// corresponding error variant will be returned.
    ///
    /// [Encoding Standard]: https://encoding.spec.whatwg.org/#names-and-labels
    pub fn parse_from_file(
        path: impl AsRef<Path>,
        encoding: Option<&str>,
    ) -> Result<Subtitles, ParsingError> {
        let mut f = fs::File::open(path)?;
        if let Some(enc) = encoding {
            let mut buffer = Vec::new();
            f.read_to_end(&mut buffer)?;
            let (cow, ..) = Encoding::for_label(enc.as_bytes())
                .ok_or(ParsingError::BadEncodingName)?
                .decode(buffer.as_slice());
            Subtitles::parse_from_str(cow[..].to_string())
        } else {
            let mut buffer = String::new();
            f.read_to_string(&mut buffer)?;
            Subtitles::parse_from_str(buffer)
        }
    }

    /// Writes the contents of this Subtitles collection to a .srt file with the correct formatting.
    ///
    /// **encoding** should either be Some("encoding-name") or None if using utf-8.
    /// For example if the file is using the ISO-8859-7 encoding (informally referred to as
    /// Latin/Greek) we could use:
    /// ```no_run
    /// use srtlib::Subtitles;
    ///
    /// let subs = Subtitles::new();
    /// // Work with the subtitles...
    /// subs.write_to_file("output.srt", Some("iso-8859-7")).unwrap();
    /// ```
    /// or the equivalent:
    /// ```no_run
    /// # use srtlib::Subtitles;
    /// # let subs = Subtitles::new();
    /// subs.write_to_file("output.srt", Some("greek")).unwrap();
    /// ```
    /// For a list of encoding names (labels) refer to the [Encoding Standard].
    ///
    /// # Errors
    ///
    /// If something goes wrong during the creation of the file using the specified path, an
    /// IOError error variant will be returned.
    ///
    /// If the encoding label provided is not one of the labels specified by the [Encoding
    /// Standard], a BadEncodingName error
    /// variant will be returned.
    ///
    /// [Encoding Standard]: https://encoding.spec.whatwg.org/#names-and-labels
    pub fn write_to_file(
        &self,
        path: impl AsRef<Path>,
        encoding: Option<&str>,
    ) -> Result<(), ParsingError> {
        let mut f = fs::File::create(path)?;
        if let Some(enc) = encoding {
            let string = &self.to_string();
            let (cow, ..) = Encoding::for_label(enc.as_bytes())
                .ok_or(ParsingError::BadEncodingName)?
                .encode(string);
            f.write_all(&cow)?;
        } else {
            f.write_all(self.to_string().as_bytes())?;
        }

        Ok(())
    }

    /// Returns the Subtitles collection as a simple vector of [`Subtitle`] structs.
    ///
    /// [`Subtitle`]: struct.Subtitle.html
    pub fn to_vec(self) -> Vec<Subtitle> {
        self.0
    }

    /// Returns the number of Subtitles in the collection.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Checks if there are no subtitles in the collection.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Adds a new subtitle at the end of the subtitles.
    pub fn push(&mut self, sub: Subtitle) {
        self.0.push(sub);
    }

    /// Sorts the subtitles in place based on their numeric counter
    pub fn sort(&mut self) {
        self.0.sort();
    }
}

impl IntoIterator for Subtitles {
    type Item = Subtitle;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'l> IntoIterator for &'l Subtitles {
    type Item = &'l Subtitle;
    type IntoIter = std::slice::Iter<'l, Subtitle>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'l> IntoIterator for &'l mut Subtitles {
    type Item = &'l mut Subtitle;
    type IntoIter = std::slice::IterMut<'l, Subtitle>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

impl<I: std::slice::SliceIndex<[Subtitle]>> Index<I> for Subtitles {
    type Output = I::Output;

    fn index(&self, i: I) -> &Self::Output {
        &self.0[i]
    }
}

impl fmt::Display for Subtitles {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.is_empty() {
            let mut s = String::new();
            for sub in &self[..self.len() - 1] {
                s.push_str(&format!("{}\n\n", &sub.to_string()));
            }
            s.push_str(&self[self.len() - 1].to_string());
            write!(f, "{}", s)
        } else {
            Ok(())
        }
    }
}

mod tests {
    #![allow(unused_imports)]
    use super::*;

    #[test]
    fn add_time_timestamp() {
        let mut timestamp = Timestamp::new(0, 0, 0, 0);
        timestamp.add_milliseconds(1200);
        assert_eq!(timestamp, Timestamp::new(0, 0, 1, 200));
        timestamp.add_seconds(65);
        assert_eq!(timestamp, Timestamp::new(0, 1, 6, 200));
        timestamp.add_minutes(122);
        assert_eq!(timestamp, Timestamp::new(2, 3, 6, 200));
        timestamp.add_hours(-1);
        assert_eq!(timestamp, Timestamp::new(1, 3, 6, 200));
        timestamp.add_seconds(-7);
        assert_eq!(timestamp, Timestamp::new(1, 2, 59, 200));
    }

    #[test]
    #[should_panic(expected = "Surpassed limits of Timestamp!")]
    fn timestamp_overflow_panic() {
        let mut timestamp = Timestamp::new(0, 0, 0, 0);
        timestamp.add_hours(255);
        timestamp.add_minutes(60);
        println!("Expected a panic, got: {}", timestamp);
    }

    #[test]
    #[should_panic(expected = "Surpassed limits of Timestamp!")]
    fn timestamp_negative_panic() {
        let mut timestamp = Timestamp::new(0, 0, 0, 0);
        timestamp.add_minutes(-10);
        println!("Expected a panic, got: {}", timestamp);
    }

    #[test]
    fn timestamp_parsing() {
        assert_eq!(
            Timestamp::parse("12:35:42,756").unwrap(),
            Timestamp::new(12, 35, 42, 756)
        );
        assert_eq!(
            Timestamp::parse("32:00:46,000").unwrap(),
            Timestamp::new(32, 0, 46, 000)
        );
    }

    #[test]
    fn timestamp_to_str() {
        assert_eq!(Timestamp::new(0, 0, 0, 0).to_string(), "00:00:00,000");
        assert_eq!(Timestamp::new(0, 1, 20, 500).to_string(), "00:01:20,500");
    }

    #[test]
    fn subtitle_parsing() {
        let input = "1\n00:00:00,000 --> 00:00:01,000\nHello world!\nNew line!";
        let result = Subtitle::new(
            1,
            Timestamp::new(0, 0, 0, 0),
            Timestamp::new(0, 0, 1, 0),
            "Hello world!\nNew line!".to_string(),
        );

        assert_eq!(Subtitle::parse(input.to_string()).unwrap(), result);
    }

    #[test]
    fn subtitle_ordering() {
        let sub1 =
            Subtitle::parse("1\n00:00:00,000 --> 00:00:02,000\nHello world!".to_string()).unwrap();
        let sub2 = Subtitle::parse("2\n00:00:02,500 --> 00:00:05,000\nTest subtitle.".to_string())
            .unwrap();
        let sub3 =
            Subtitle::parse("2\n00:00:03,500 --> 00:00:06,000\nTest subtitle two.".to_string())
                .unwrap();

        assert!(sub1 < sub2);
        assert!(sub2 < sub3);
    }

    #[test]
    fn add_time_subtitle() {
        let mut sub =
            Subtitle::parse("1\n00:00:00,000 --> 00:00:02,000\nHello world!".to_string()).unwrap();
        sub.add_seconds(10);
        assert_eq!(
            sub.to_string(),
            "1\n00:00:10,000 --> 00:00:12,000\nHello world!"
        );
        sub.add_seconds(110);
        assert_eq!(
            sub.to_string(),
            "1\n00:02:00,000 --> 00:02:02,000\nHello world!"
        );
        let t1 = Timestamp::new(0, 0, 0, 0);
        let t2 = Timestamp::new(1, 20, 0, 0);
        sub.add(&t1);
        assert_eq!(
            sub.to_string(),
            "1\n00:02:00,000 --> 00:02:02,000\nHello world!"
        );
        sub.add(&t2);
        assert_eq!(
            sub.to_string(),
            "1\n01:22:00,000 --> 01:22:02,000\nHello world!"
        );
    }

    #[test]
    fn sub_to_string() {
        let input = Subtitle::new(
            1,
            Timestamp::new(0, 0, 0, 0),
            Timestamp::new(0, 0, 1, 0),
            "Hello world!\nNew line!".to_string(),
        );
        let result = "1\n00:00:00,000 --> 00:00:01,000\nHello world!\nNew line!";

        assert_eq!(input.to_string(), result);
    }

    #[test]
    fn subtitles_from_str_parsing() {
        let subs = "1\n00:00:00,000 --> 00:00:01,000\nHello world!\nExtra!\n\n\
                    2\n00:00:01,500 --> 00:00:02,500\nThis is a subtitle!";

        let parsed_subs = Subtitles::parse_from_str(subs.to_string()).unwrap();
        assert_eq!(
            parsed_subs[0],
            Subtitle::new(
                1,
                Timestamp::new(0, 0, 0, 0),
                Timestamp::new(0, 0, 1, 0),
                "Hello world!\nExtra!".to_string()
            )
        );
        assert_eq!(
            parsed_subs[1],
            Subtitle::new(
                2,
                Timestamp::new(0, 0, 1, 500),
                Timestamp::new(0, 0, 2, 500),
                "This is a subtitle!".to_string()
            )
        );
    }

    #[test]
    fn sort_subtitles() {
        let subs = "2\n00:00:01,500 --> 00:00:02,500\nThis is a subtitle!\n\n\
                    1\n00:00:00,000 --> 00:00:01,000\nHello world!\nExtra!\n\n\
                    3\n00:00:02,500 --> 00:00:03,000\nFinal subtitle.\n";

        let mut parsed_subs = Subtitles::parse_from_str(subs.to_string()).unwrap();
        parsed_subs.sort();

        let true_sort = "1\n00:00:00,000 --> 00:00:01,000\nHello world!\nExtra!\n\n\
                         2\n00:00:01,500 --> 00:00:02,500\nThis is a subtitle!\n\n\
                         3\n00:00:02,500 --> 00:00:03,000\nFinal subtitle.\n";
        let sorted_subs = Subtitles::parse_from_str(true_sort.to_string()).unwrap();

        assert_eq!(parsed_subs, sorted_subs);
    }

    #[test]
    fn empty_subtitles_display() {
        let out = Subtitles::new().to_string();
        assert_eq!(out, String::new());
    }

    #[test]
    fn empty_subtitles_parse() {
        let subs = Subtitles::parse_from_str(String::new()).expect("Failed to parse empty subs");
        assert_eq!(subs.len(), 0);
    }

    #[test]
    fn subtitle_with_position_information() {
        let input = "1\n00:00:07,001 --> 00:00:09,015 position:50,00%,middle align:middle size:80,00% line:84,67%\nThis is a subtitle text";
        let result = Subtitle::new(
            1,
            Timestamp::new(0, 0, 7, 1),
            Timestamp::new(0, 0, 9, 15),
            "This is a subtitle text".to_string(),
        );

        assert_eq!(Subtitle::parse(input.to_string()).unwrap(), result);
    }
}
