/** XLSForm authoring types. Collection coverage is separate from definition support. */
export const questionTypes = [
  {
    group: 'Written answers',
    items: [
      ['text', 'Text'],
      ['integer', 'Whole number'],
      ['decimal', 'Decimal'],
      ['range', 'Scale / range'],
      ['note', 'Instruction / note'],
      ['acknowledge', 'Acknowledgement']
    ]
  },
  {
    group: 'Choices',
    items: [
      ['select_one', 'Single choice'],
      ['select_multiple', 'Multiple choice'],
      ['rank', 'Ranking'],
      ['select_one_from_file', 'Single choice from dataset'],
      ['select_multiple_from_file', 'Multiple choice from dataset']
    ]
  },
  {
    group: 'Dates and time',
    items: [
      ['date', 'Date'],
      ['time', 'Time'],
      ['dateTime', 'Date and time']
    ]
  },
  {
    group: 'Location and media',
    items: [
      ['geopoint', 'GPS point'],
      ['geotrace', 'GPS line / route'],
      ['geoshape', 'GPS area'],
      ['image', 'Image / signature / drawing'],
      ['audio', 'Audio'],
      ['video', 'Video'],
      ['file', 'File attachment'],
      ['barcode', 'Barcode / QR code'],
      ['osm', 'OpenStreetMap feature']
    ]
  },
  {
    group: 'Logic and metadata',
    items: [
      ['calculate', 'Calculated value'],
      ['hidden', 'Hidden field'],
      ['start', 'Start time'],
      ['end', 'End time'],
      ['today', 'Today'],
      ['deviceid', 'Device ID'],
      ['username', 'Enumerator username'],
      ['email', 'Enumerator email'],
      ['phonenumber', 'Phone number metadata'],
      ['audit', 'Audit log'],
      ['start-geopoint', 'Starting location'],
      ['background-geopoint', 'Background location'],
      ['background-audio', 'Background audio'],
      ['xml-external', 'External XML dataset']
    ]
  }
];
export const advancedColumns = [
  'appearance',
  'parameters',
  'choice_filter',
  'repeat_count',
  'read_only',
  'trigger',
  'bind::type',
  'bind::odk:accuracyThreshold',
  'bind::odk:unacceptableAccuracyThreshold',
  'media::image',
  'media::audio',
  'media::video'
];
