extern crate lazytiff;

use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    
    let filename = Path::new(&args[1]);
    
    let tiff_file = File::open(filename)?;
    let mut tiff_reader = lazytiff::TiffReader::new(tiff_file).unwrap();
    tiff_reader.read_all_ifds()?;
    
    // All tags listed in the TIFF 6.0 spec
    let mut tag_names: HashMap<u16, String> = HashMap::new();
    tag_names.insert(254, "NewSubfileType".to_string());
    tag_names.insert(255, "SubfileType".to_string());
    tag_names.insert(256, "ImageWidth".to_string());
    tag_names.insert(257, "ImageLength".to_string());
    tag_names.insert(258, "BitsPerSample".to_string());
    tag_names.insert(259, "Compression".to_string());
    tag_names.insert(262, "PhotometricInterpretation".to_string());
    tag_names.insert(263, "Threshholding".to_string());
    tag_names.insert(264, "CellWidth".to_string());
    tag_names.insert(265, "CellLength".to_string());
    tag_names.insert(266, "FillOrder".to_string());
    tag_names.insert(269, "DocumentName".to_string());
    tag_names.insert(270, "ImageDescription".to_string());
    tag_names.insert(271, "Make".to_string());
    tag_names.insert(272, "Model".to_string());
    tag_names.insert(273, "StripOffsets".to_string());
    tag_names.insert(274, "Orientation".to_string());
    tag_names.insert(277, "SamplesPerPixel".to_string());
    tag_names.insert(278, "RowsPerStrip".to_string());
    tag_names.insert(279, "StripByteCounts".to_string());
    tag_names.insert(280, "MinSampleValue".to_string());
    tag_names.insert(281, "MaxSampleValue".to_string());
    tag_names.insert(282, "XResolution".to_string());
    tag_names.insert(283, "YResolution".to_string());
    tag_names.insert(284, "PlanarConfiguration".to_string());
    tag_names.insert(285, "PageName".to_string());
    tag_names.insert(286, "XPosition".to_string());
    tag_names.insert(287, "YPosition".to_string());
    tag_names.insert(288, "FreeOffsets".to_string());
    tag_names.insert(289, "FreeByteCounts".to_string());
    tag_names.insert(290, "GrayResponseUnit".to_string());
    tag_names.insert(291, "GrayResponseCurve".to_string());
    tag_names.insert(292, "T4Options".to_string());
    tag_names.insert(293, "T6Options".to_string());
    tag_names.insert(296, "ResolutionUnit".to_string());
    tag_names.insert(297, "PageNumber".to_string());
    tag_names.insert(301, "TransferFunction".to_string());
    tag_names.insert(305, "Software".to_string());
    tag_names.insert(306, "DateTime".to_string());
    tag_names.insert(315, "Artist".to_string());
    tag_names.insert(316, "HostComputer".to_string());
    tag_names.insert(317, "Predictor".to_string());
    tag_names.insert(318, "WhitePoint".to_string());
    tag_names.insert(319, "PrimaryChromaticities".to_string());
    tag_names.insert(320, "ColorMap".to_string());
    tag_names.insert(321, "HalftoneHints".to_string());
    tag_names.insert(322, "TileWidth".to_string());
    tag_names.insert(323, "TileLength".to_string());
    tag_names.insert(324, "TileOffsets".to_string());
    tag_names.insert(325, "TileByteCounts".to_string());
    tag_names.insert(332, "InkSet".to_string());
    tag_names.insert(333, "InkNames".to_string());
    tag_names.insert(334, "NumberOfInks".to_string());
    tag_names.insert(336, "DotRange".to_string());
    tag_names.insert(337, "TargetPrinter".to_string());
    tag_names.insert(338, "ExtraSamples".to_string());
    tag_names.insert(339, "SampleFormat".to_string());
    tag_names.insert(340, "SMinSampleValue".to_string());
    tag_names.insert(341, "SMaxSampleValue".to_string());
    tag_names.insert(342, "TransferRange".to_string());
    tag_names.insert(512, "JPEGProc".to_string());
    tag_names.insert(513, "JPEGInterchangeFormat".to_string());
    tag_names.insert(514, "JPEGInterchangeFormatLngth".to_string());
    tag_names.insert(515, "JPEGRestartInterval".to_string());
    tag_names.insert(517, "JPEGLosslessPredictors".to_string());
    tag_names.insert(518, "JPEGPointTransforms".to_string());
    tag_names.insert(519, "JPEGQTables".to_string());
    tag_names.insert(520, "JPEGDCTables".to_string());
    tag_names.insert(521, "JPEGACTables".to_string());
    tag_names.insert(529, "YCbCrCoefficients".to_string());
    tag_names.insert(530, "YCbCrSubsampling".to_string());
    tag_names.insert(531, "YCbCrPositioning".to_string());
    tag_names.insert(532, "ReferenceBlackWhite".to_string());
    tag_names.insert(33432, "Copyright".to_string());
    
    let mut all_tags: Vec<u16> = tag_names.keys().map(|tag| *tag).collect();
    all_tags.sort();
    
    println!("{}", filename.file_name().unwrap().to_string_lossy());
    
    let num_subfiles = tiff_reader.subfiles.len();
    for (i, subfile) in tiff_reader.subfiles.iter().enumerate() {
        let is_last_subfile = i == num_subfiles - 1;
        let hierarchy_prefix = if is_last_subfile {" └─"} else {" ├─"};
        println!("{} Subfile {}", hierarchy_prefix, i);
        
        let mut present_fields = Vec::new();
        for tag in &all_tags {
            if let Some(field) = subfile.get_field(*tag) {
                present_fields.push((*tag, field.get_value_if_local()))
            }
        }
        
        for (j, field) in present_fields.iter().enumerate() {
            let is_last_field = j == present_fields.len() - 1;
            let hierarchy_prefix = if is_last_subfile {
                if is_last_field {"     └─"} else {"     ├─"}
            } else {
                if is_last_field {" │   └─"} else {" │   ├─"}
            };
            
            let (tag, field_value_opt) = field;
            let tag_name = tag_names.get(tag).unwrap();
            let field_value_text = match field_value_opt {
                Some(value) => format!("{:?}", value),
                None => "(not loaded)".to_string(),
            };
            
            println!("{} {}: {}", hierarchy_prefix, tag_name, field_value_text);
        }
    }
    
    Ok(())
}
