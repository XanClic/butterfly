#!/usr/bin/ruby

require 'json'

def parse_int(str)
    begin
        Integer(str)
    rescue
        nil
    end
end

def parse_offset(ofs)
    base = {}
    if ofs =~ /\s*align_up\s*/
        split = ofs.split(/\s*align_up\s*/)
        ofs = split[0]
        base[:align] = {
            type: :Up,
            value: parse_int(split[1])
        }
    elsif ofs =~ /\s*align_down\s*/
        split = ofs.split(/\s*align_down\s*/)
        ofs = split[0]
        base[:align] = {
            type: :Down,
            value: parse_int(split[1])
        }
    elsif ofs =~ /\s*align\s*/
        split = ofs.split(/\s*align\s*/)
        ofs = split[0]
        base[:align] = {
            type: :Near,
            value: parse_int(split[1])
        }
    end
    if ofs.include?('+')
        ofs = ofs.split('+')
    else
        ofs = [ofs]
    end
    return {
        type: :Abs,
        offset: ofs.map { |o|
            parse_int(o) ? parse_int(o) : o
        }
    }
end

def parse_conditional(c)
    if c.split('==')[1]
        sides = c.split('==')
        return {
            variable: sides[0].strip,
            comparison: :Equals,
            value: parse_int(sides[1]) ? parse_int(sides[1]) : sides[1],
        }
    elsif c.split('!=')[1]
        sides = c.split('!=')
        return {
            variable: sides[0].strip,
            comparison: :NotEquals,
            value: parse_int(sides[1]) ? parse_int(sides[1]) : sides[1],
        }
    elsif c.split('<=')[1]
        sides = c.split('<=')
        return {
            variable: sides[0].strip,
            comparison: :LessThanOrEqual,
            value: parse_int(sides[1]) ? parse_int(sides[1]) : sides[1],
        }
    elsif c.split('>=')[1]
        sides = c.split('>=')
        return {
            variable: sides[0].strip,
            comparison: :GreaterThanOrEqual,
            value: parse_int(sides[1]) ? parse_int(sides[1]) : sides[1],
        }
    elsif c.split('<')[1]
        sides = c.split('<')
        return {
            variable: sides[0].strip,
            comparison: :LessThan,
            value: parse_int(sides[1]) ? parse_int(sides[1]) : sides[1],
        }
    elsif c.split('>')[1]
        sides = c.split('>')
        return {
            variable: sides[0].strip,
            comparison: :GreaterThan,
            value: parse_int(sides[1]) ? parse_int(sides[1]) : sides[1],
        }
    else
        raise "Unknown condition #{c}"
    end
end

argv = ARGV.to_a
name = argv.shift
tplt_path = argv.shift
json_path = argv.shift

if !name || !tplt_path || !json_path || !argv.empty?
    $stderr.puts('Usage: template-to-json.rb <name> <template> <json>')
    exit 1
end

template = IO.readlines(tplt_path)
output = {
    name: name,
    content: [],
}

def walk(hash, path)
    hash = hash[:content]
    path.each do |e|
        hash = hash.find { |element|
            (element[:type] == :Folder || element[:type] == :Loop) &&
                element[:name] == e
        }
        if !hash
            raise "Failed to resolve path #{path.inspect}"
        end
        hash = hash[:content]
    end
    hash
end

current_path = []
template.each do |line|
    indentation_m = line.match(/^\s*/)[0]
    indentation = indentation_m.count(' ') / 4 + indentation_m.count("\t")
    line.strip!

    next if line.empty?

    if indentation == 0
        current_path = []
    else
        current_path = current_path[0..(indentation-1)]
    end
    current_dir = walk(output, current_path)

    orig_line = line

    conditional = line.split(' if ')
    line = conditional[0].strip
    conditional = conditional[1]
    conditional.strip! if conditional

    if line =~ /^\w+:$/
        folder_name = line[0..-2]
        element = {
            type: :Folder,
            name: folder_name,
            content: [],
        }
        if conditional
            element[:condition] = parse_conditional(conditional)
        end
        current_dir << element
        current_path << folder_name
    elsif line =~ /^loop\s*\/.*:$/
        loop_spec = line.split(' ')
        loop_name = loop_spec[-1][0..-2].strip
        loop_spec = (loop_spec[0..-2] * ' ').split('/')

        if loop_spec.shift.strip != 'loop'
            raise 'what'
        end
        base = loop_spec.shift.strip
        loop_incr = loop_spec.shift.strip
        loop_cond = loop_spec.shift.strip

        element = {
            type: :Loop,
            name: loop_name,
            base_offset: parse_offset(base),
            base_adjust: parse_offset(loop_incr),
            condition: parse_conditional(loop_cond),
            content: [],
        }
        current_dir << element
        current_path << loop_name
    else
        if conditional
            raise "#{orig_line}: Conditionals can only be used for folders"
        end

        match = /^([^:]+)\s*:\s*(\w+)\s*:\s*(.*)$/.match(line)
        if !match
            raise "#{orig_line}: ?"
        end
        offset = match[1]
        type = match[3].strip

        element = {
            type: :Value,
            name: match[2],
        }

        element[:offset] = parse_offset(offset)

        as_split = type.split(' as ')
        basic_type = as_split[0].strip.split('/')
        display = as_split[1]
        display.strip! if display

        if basic_type[0] == 'str'
            element[:kind] = {
                type: :String
            }
            element[:kind][:length] = {
                type: :Fixed,
                length: parse_int(basic_type[1]) ? parse_int(basic_type[1]) : basic_type[1],
            }
            element[:kind][:encoding] = basic_type[2]

            if display
                raise "#{orig_line}: Unexpected display modification #{display}"
            end
        else
            if basic_type[0][0] == 'i'
                element[:kind] = {
                    type: :Integer,
                    sign: :SignTwoCompl,
                }
            elsif basic_type[0][0] == 'u'
                element[:kind] = {
                    type: :Integer,
                    sign: :Unsigned,
                }
            else
                raise "#{orig_line}: Unknown basic type #{basic_type[0]}"
            end

            width = parse_int(basic_type[0][1..-1])
            if !width
                raise "#{orig_line}: Unknown basic type #{basic_type[0]}"
            end
            if width % 8 != 0
                raise "#{orig_line}: Width must be a multiple of 8"
            end
            if width > 64 || width < 8
                raise "#{orig_line}: Width must be between 8 and 64"
            end

            element[:kind][:width] = width / 8
            if basic_type[1]
                if basic_type[1] == 'le'
                    element[:kind][:endianness] = :Little
                elsif basic_type[1] == 'be'
                    element[:kind][:endianness] = :Big
                else
                    raise "#{orig_line}: Unknown type modifier #{basic_type[1]}"
                end
            else
                element[:kind][:endianness] = :Little
            end
            if basic_type[2]
                raise "#{orig_line}: Unexpected type modifier #{basic_type[2]}"
            end

            if !display
                element[:kind][:base] = 16
            else
                case display
                when 'bin'
                    element[:kind][:base] = 2
                when 'oct'
                    element[:kind][:base] = 8
                when 'dec'
                    element[:kind][:base] = 10
                when 'hex'
                    element[:kind][:base] = 16
                else
                    base = parse_int(display)
                    if base
                        element[:kind][:base] = base
                    else
                        raise "#{orig_line}: Unknown display modifier #{display}"
                    end
                end
            end
        end

        current_dir << element
    end
end

IO.write(json_path, JSON.pretty_generate(output))
